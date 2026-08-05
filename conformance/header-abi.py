#!/usr/bin/env python3
"""What a C header offers a consumer, in three forms a shell can diff.

    header-abi.py inventory <header> [-I<dir> ...]
    header-abi.py layout    <header> [-I<dir> ...]
    header-abi.py compat    <ours> <theirs> [-I<dir> ...]

WHY A COMPILER AND NOT A REGEX. The two headers being compared are written by
different hands — one by NetBSD, one by cbindgen — so anything comparing them
as text measures the hands rather than the contract. `struct lineinfo *` and
`LineInfo *` are one type; `int rl_point, rl_end;` is two declarations;
`__attribute__((__format__(...)))` is neither; and `int32_t` and `int` are the
same type on every target this library builds for. Only a C front end knows
which is which, so this drives one: clang for the declarations, cpp for the
macros, and the compiler itself for every question about a type.

`inventory` answers WHAT IS DECLARED — names, and the structure a consumer can
see. One line per declaration, sorted, so `comm(1)` can subtract two headers:

    macro NAME = TOKENS              object-like #define
    macro NAME(A, B) = TOKENS        function-like #define
    struct TAG                       a record declared and left incomplete
    struct TAG fields: a b c         a record definition, in field order
    enum TAG { A B C }               an enumeration
    typedef NAME                     a typedef
    typedef NAME fields: a b c       a typedef naming a record
    var NAME                         an object with external linkage
    func NAME / func NAME variadic   a function

It carries no type SPELLINGS, deliberately. Two headers may spell one type
differently and still declare the same thing, and a diff that reported that as
a defect would train its reader to ignore it. Types are checked by `compat`.

`layout` answers WHERE THE BYTES ARE, by compiling and running a program that
prints `sizeof`, `_Alignof` and `offsetof` for every record the header
completes. This is the check the whole stage exists for: a field in the wrong
order inside `LineInfo` or `HistEvent` breaks every consumer that reads it and
leaves the symbol table byte-identical.

`compat` answers ARE THE TYPES THE SAME, by generating one
`__builtin_types_compatible_p` assertion per declaration — written with the
type as the ORIGINAL header spells it, compiled against OURS. So the question
asked is exactly the right one: a consumer who learned the API from libedit's
header, and writes `rl_hook_func_t *`, gets a type our header agrees with. Any
assertion that fails is named, with both spellings, and the failure is a
FAILURE — never a spelling difference, because the compiler was asked about
the type and not about the text.
"""

import json
import os
import re
import subprocess
import sys
import tempfile

CC = "gcc"
CLANG = "clang"


def die(msg):
    sys.stderr.write("header-abi: %s\n" % msg)
    sys.exit(2)


def run(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def probe_file(td, header, body=""):
    path = os.path.join(td, "probe.c")
    with open(path, "w") as f:
        f.write('#include "%s"\n%s' % (header, body))
    return path


# ---------------------------------------------------------------------------
# Macros.
# ---------------------------------------------------------------------------


def norm_macro_value(v):
    """An integer literal is its value, however it is written.

    `0x20` and `32` are the same constant; so are `'\\1'` and `1`. Reporting a
    difference there would be reporting how the two authors chose to write a
    number. Anything that is not a single integer or character literal is left
    exactly as written, so `CTRL('G')` and `((c) & 037)` are compared as text.
    """
    s = v.strip()
    m = re.fullmatch(r"[-+]?(0[xX][0-9a-fA-F]+|0[0-7]*|[1-9][0-9]*)[uUlL]*", s)
    if m:
        return str(int(s.rstrip("uUlL"), 0))
    m = re.fullmatch(r"'\\([0-7]{1,3})'", s)
    if m:
        return str(int(m.group(1), 8))
    m = re.fullmatch(r"'(.)'", s)
    if m:
        return str(ord(m.group(1)))
    return re.sub(r"\s+", " ", s)


def macros(header, incdirs):
    """The macros this header itself defines.

    `cpp -dD` interleaves the `#define` directives with the line markers that
    say which file the preprocessor is in, so the ones belonging to this header
    can be told from the ones its own `#include`s brought in.
    """
    with tempfile.TemporaryDirectory() as td:
        probe = probe_file(td, header)
        p = run([CC, "-E", "-dD"] + ["-I" + d for d in incdirs] + [probe])
    if p.returncode != 0:
        die("preprocessing %s failed:\n%s" % (header, p.stderr))
    want = os.path.realpath(header)
    cur, out = None, []
    for line in p.stdout.splitlines():
        m = re.match(r'# \d+ "([^"]*)"', line)
        if m:
            cur = os.path.realpath(m.group(1))
            continue
        if not line.startswith("#define") or cur != want:
            continue
        body = line[len("#define") :].strip()
        m = re.match(r"([A-Za-z_]\w*)(\([^)]*\))?\s*(.*)$", body, re.S)
        if not m:
            continue
        name, params, value = m.group(1), m.group(2) or "", m.group(3)
        if params:
            params = "(" + ", ".join(x.strip() for x in params[1:-1].split(",")) + ")"
        out.append("macro %s%s = %s" % (name, params, norm_macro_value(value)))
    return out


# ---------------------------------------------------------------------------
# Declarations, via clang's AST.
# ---------------------------------------------------------------------------


def norm_type(t):
    """Whitespace only. Nothing here decides that two types are equal."""
    t = re.sub(r"\s+", " ", t).strip()
    t = re.sub(r"\s*\*\s*", "*", t)
    return re.sub(r"([^\s*(])\*", r"\1 *", t)


def parse(header, incdirs):
    """Top-level declarations this header contributes, in source order.

    clang's JSON emits `loc.file` only when the file CHANGES, so the current
    file is carried forward: a node with no `file` is in whatever file the
    previous node was in.
    """
    with tempfile.TemporaryDirectory() as td:
        probe = probe_file(td, header)
        p = run(
            [CLANG, "-fsyntax-only", "-Xclang", "-ast-dump=json"]
            + ["-I" + d for d in incdirs]
            + [probe]
        )
    if p.returncode != 0:
        die("clang could not parse %s:\n%s" % (header, p.stderr))
    root = json.loads(p.stdout)

    want = os.path.realpath(header)
    cur = None
    items, anon = [], None
    for node in root.get("inner", []):
        loc = node.get("loc", {})
        if "file" in loc:
            cur = os.path.realpath(loc["file"])
        if cur != want:
            continue
        kind, name = node.get("kind"), node.get("name")

        if kind == "RecordDecl":
            fields = None
            if node.get("completeDefinition"):
                fields = [
                    (f.get("name", "?"), norm_type(f["type"]["qualType"]))
                    for f in node.get("inner", [])
                    if f.get("kind") == "FieldDecl"
                ]
            if not name:
                # Held for the typedef that is about to name it.
                anon = fields
                continue
            items.append(("record", node.get("tagUsed", "struct"), name, fields))
        elif kind == "TypedefDecl":
            items.append(("typedef", name, norm_type(node["type"]["qualType"]), anon))
            anon = None
        elif kind == "VarDecl":
            items.append(("var", name, norm_type(node["type"]["qualType"]), None))
        elif kind == "FunctionDecl":
            items.append(("func", name, norm_type(node["type"]["qualType"]), None))
        elif kind == "EnumDecl":
            names = [
                i.get("name")
                for i in node.get("inner", [])
                if i.get("kind") == "EnumConstantDecl"
            ]
            items.append(("enum", name or "<anonymous>", names, None))
        if kind != "RecordDecl" or name:
            anon = None
    return items


def records(items):
    """Every record a consumer can read, keyed by the name they would write.

    Returns {name: [(field, type), ...]}. A typedef of a record is preferred
    over the tag, because that is what the header's own signatures use and
    what a consumer therefore writes; the tag is kept as well when it has one.
    """
    by_tag = {n: f for (k, _t, n, f) in items if k == "record" and f}
    out = dict(by_tag)
    for it in items:
        if it[0] != "typedef":
            continue
        _k, name, aliased, anon_fields = it
        m = re.fullmatch(r"(?:struct|union) (\w+)", aliased)
        if m and m.group(1) in by_tag:
            out[name] = by_tag[m.group(1)]
        elif anon_fields:
            out[name] = anon_fields
    return out


def inventory(header, incdirs):
    items = parse(header, incdirs)
    recs = records(items)
    out = list(macros(header, incdirs))
    for it in items:
        kind = it[0]
        if kind == "record":
            _k, tag, name, fields = it
            if fields is None:
                out.append("%s %s" % (tag, name))
            else:
                out.append(
                    "%s %s fields: %s" % (tag, name, " ".join(f for f, _ in fields))
                )
        elif kind == "typedef":
            name = it[1]
            if name in recs:
                out.append("typedef %s fields: %s" % (name, " ".join(f for f, _ in recs[name])))
            else:
                out.append("typedef %s" % name)
        elif kind == "var":
            out.append("var %s" % it[1])
        elif kind == "func":
            out.append("func %s%s" % (it[1], " variadic" if ", ...)" in it[2] else ""))
        elif kind == "enum":
            out.append("enum %s { %s }" % (it[1], " ".join(it[2])))
    return sorted(set(out))


# ---------------------------------------------------------------------------
# Layout: where the bytes are, measured rather than inferred.
# ---------------------------------------------------------------------------


def layout(header, incdirs):
    items = parse(header, incdirs)
    recs = records(items)
    # Only the names a consumer writes: a tag that a typedef also covers would
    # print the same numbers twice under two names.
    tags = {n for (k, _t, n, f) in items if k == "record" and f}
    covered = set()
    for it in items:
        if it[0] == "typedef":
            m = re.fullmatch(r"(?:struct|union) (\w+)", it[2])
            if m:
                covered.add(m.group(1))
    names = sorted(n for n in recs if not (n in tags and n in covered))

    body = ["#include <stddef.h>", "#include <stdio.h>", "int main(void) {"]
    for n in names:
        ty = ("struct %s" % n) if n in tags else n
        body.append(
            '  printf("layout %s size=%%zu align=%%zu\\n", sizeof(%s), _Alignof(%s));'
            % (n, ty, ty)
        )
        for f, _t in recs[n]:
            body.append(
                '  printf("layout %s.%s offset=%%zu size=%%zu\\n", offsetof(%s, %s), sizeof(((%s *)0)->%s));'
                % (n, f, ty, f, ty, f)
            )
    body.append("  return 0;\n}")

    with tempfile.TemporaryDirectory() as td:
        src = probe_file(td, header, "\n".join(body) + "\n")
        exe = os.path.join(td, "layout")
        p = run([CC, "-o", exe] + ["-I" + d for d in incdirs] + [src])
        if p.returncode != 0:
            die("could not build the layout probe for %s:\n%s" % (header, p.stderr))
        p = run([exe])
        if p.returncode != 0:
            die("the layout probe for %s did not run:\n%s" % (header, p.stderr))
    return sorted(p.stdout.splitlines())


# ---------------------------------------------------------------------------
# Compatibility: are the types the same, asked of a C compiler.
# ---------------------------------------------------------------------------

# A type clang printed for an anonymous record cannot be written down again:
# writing `struct { int length; }` in the probe would declare a NEW type,
# incompatible with everything including itself. Those declarations are
# checked by `inventory` (the field names) and `layout` (the bytes) instead.
UNWRITABLE = re.compile(r"\((?:unnamed|anonymous)")


def compat(ours, theirs, incdirs):
    theirs_items = parse(theirs, incdirs)
    theirs_recs = records(theirs_items)
    ours_inv = set(inventory(ours, incdirs))

    def have(line):
        return any(x == line or x.startswith(line + " ") for x in ours_inv)

    checks = []  # (label, expression-source, their spelling)
    for it in theirs_items:
        kind = it[0]
        if kind in ("func", "var"):
            name, ty = it[1], it[2]
            if not have("%s %s" % (kind, name)) or UNWRITABLE.search(ty):
                continue
            checks.append(
                (
                    "%s %s" % (kind, name),
                    "__builtin_types_compatible_p(__typeof__(%s), %s)" % (name, ty),
                    ty,
                )
            )
        elif kind == "typedef":
            name, ty, anon = it[1], it[2], it[3]
            # A typedef naming an anonymous record: clang prints the typedef's
            # own name for it, and writing that down again would name a
            # different type. Checked by `inventory` and `layout` instead.
            if anon is not None:
                continue
            if not have("typedef %s" % name) or UNWRITABLE.search(ty):
                continue
            checks.append(
                (
                    "typedef %s" % name,
                    "__builtin_types_compatible_p(%s, %s)" % (name, ty),
                    ty,
                )
            )
    for name, fields in sorted(theirs_recs.items()):
        if not have("typedef %s" % name):
            continue
        for f, ty in fields:
            if UNWRITABLE.search(ty):
                continue
            checks.append(
                (
                    "field %s.%s" % (name, f),
                    "__builtin_types_compatible_p(__typeof__(((%s *)0)->%s), %s)"
                    % (name, f, ty),
                    ty,
                )
            )

    lines = []
    at = {}
    for i, (label, expr, _ty) in enumerate(checks):
        lines.append('_Static_assert(%s, "%s");' % (expr, label))
        at[len(lines) + 1] = i  # +1 for the #include line the probe starts with

    with tempfile.TemporaryDirectory() as td:
        src = probe_file(td, ours, "\n".join(lines) + "\n")
        p = run(
            [CLANG, "-fsyntax-only", "-ferror-limit=0"]
            + ["-I" + d for d in incdirs]
            + [src]
        )

    bad = set()
    for m in re.finditer(r"probe\.c:(\d+):\d+: error:", p.stderr):
        bad.add(int(m.group(1)))
    unattributed = [n for n in bad if n not in at]
    out = []
    for n in sorted(bad):
        if n not in at:
            continue
        label, _expr, ty = checks[at[n]]
        out.append("%s :: incompatible with the original's `%s`" % (label, ty))
    if unattributed:
        out.append(
            "the probe failed on %d line(s) that are not assertions; "
            "the header may not compile. clang said:\n%s"
            % (len(unattributed), p.stderr.strip())
        )
    return out, len(checks)


# ---------------------------------------------------------------------------


def main():
    argv = sys.argv[1:]
    if not argv:
        die(__doc__.strip().splitlines()[0])
    cmd = argv[0]
    incdirs = [a[2:] for a in argv if a.startswith("-I")]
    pos = [os.path.abspath(a) for a in argv[1:] if not a.startswith("-")]

    if cmd == "inventory":
        for line in inventory(pos[0], incdirs):
            print(line)
    elif cmd == "layout":
        for line in layout(pos[0], incdirs):
            print(line)
    elif cmd == "compat":
        failures, total = compat(pos[0], pos[1], incdirs)
        for f in failures:
            print(f)
        print("# %d type assertions, %d failed" % (total, len(failures)))
        sys.exit(1 if failures else 0)
    else:
        die("unknown subcommand %r" % cmd)


if __name__ == "__main__":
    main()
