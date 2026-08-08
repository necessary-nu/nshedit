//! Ported from `src/keymacro.c`; rules live in
//! `docs/spec/port/src/keymacro.md`.

use std::process;

use crate::compat::chartype::{
    VISUAL_WIDTH_MAX, ct_encode_char, ct_encode_string, ct_visual_char, upto_nul,
};
use crate::compat::el::{EL_BUFSIZ, EditLine, ElActionT};
use crate::compat::fcns::ED_SEQUENCE_LEAD_IN;
use crate::compat::map::{ElMapCurrent, N_KEYS};
use crate::compat::read::el_wgetc;

/// C: `#define XK_CMD 0` — the node's `val` is an editor action.
pub(crate) const XK_CMD: i32 = 0;
/// C: `#define XK_STR 1` — the node's `val` is a macro expansion string.
pub(crate) const XK_STR: i32 = 1;
/// C: `#define XK_NOD 2` — no binding here. The node exists only because it
/// lies on the path of some longer key.
pub(crate) const XK_NOD: i32 = 2;

/// C: `#define KEY_BUFSIZ EL_BUFSIZ` — the size of `el_keymacro.buf`,
/// counted in `wchar_t`.
const KEY_BUFSIZ: usize = EL_BUFSIZ;

// [spec:libedit:def:keymacro.keymacro-value-t]
/// What a key sequence resolves to.
///
/// C: a `union { el_action_t cmd; wchar_t *str; }` discriminated by the
/// neighbouring `type` field (`XK_CMD` 0, `XK_STR` 1, `XK_NOD` 2). A C union
/// with an owning pointer in it has no safe Rust spelling, and the tag is
/// already there, so it becomes an enum. The `type` field stays where the C
/// put it — nothing is collapsed — and `XK_NOD` is the C's "the union holds
/// a NULL `str`" state, which is `Str` with an empty buffer.
/// `Clone` is not the C's — the C copies the union by assignment. It is here
/// because the C's own call sites borrow this out of the `EditLine` while also
/// passing the `EditLine` mutably (`terminal_reset_arrow` does exactly that),
/// which Rust cannot express; cloning into a local first is the translation.
#[derive(Clone)]
pub enum KeymacroValueT {
    /// C: `el_action_t cmd` — read when `type == XK_CMD`.
    Cmd(ElActionT),
    /// C: `wchar_t *str` — read when `type == XK_STR`; owned, and freed by
    /// `keymacro_end`.
    Str(Vec<u32>),
}

// [spec:libedit:def:keymacro.keymacro-node-t]
/// A node of `el->el_keymacro.map`, the trie of bound key sequences.
///
/// `next` and `sibling` are single-owner links — a node is reachable from
/// exactly one parent — so `Box` is both safe and literal here, unlike the
/// history list.
pub struct KeymacroNodeT {
    /// C: `wchar_t ch` — single character of the key.
    pub ch: u32,
    /// Node type: `XK_CMD`, `XK_STR` or `XK_NOD`.
    pub r#type: i32,
    /// Command code or macro string, if this is a leaf.
    pub val: KeymacroValueT,
    /// Next char of this key.
    pub next: Option<Box<KeymacroNodeT>>,
    /// Another key with the same prefix.
    pub sibling: Option<Box<KeymacroNodeT>>,
}

// [spec:libedit:def:keymacro.el-keymacro-t]
/// The key-macro trie and its scratch space.
pub struct ElKeymacroT {
    /// C: `wchar_t *buf` — key print buffer, owned.
    pub buf: Vec<u32>,
    /// C: `keymacro_node_t *map` — the key map, owned.
    pub map: Option<Box<KeymacroNodeT>>,
    /// Local conversion buffer: where `keymacro_get` leaves the value it
    /// found. Its discriminant is `keymacro_get`'s return value, not a
    /// stored `type` field.
    pub val: KeymacroValueT,
}

// [spec:libedit:def:keymacro.keymacro-init-fn]
// [spec:libedit:sem:keymacro.keymacro-init-fn]
/// Initialize the key maps. 0 on success, -1 if the print buffer could not
/// be allocated.
pub(crate) fn keymacro_init(el: &mut EditLine) -> i32 {
    // Step 1: `el_calloc(KEY_BUFSIZ, sizeof(wchar_t))`, fallibly —
    // `Vec::resize` alone would abort the process on OOM and there would be
    // no -1 left to return. On failure `buf` keeps its previous (empty)
    // value, which is the C's NULL: every reader below tests for it.
    let mut buf: Vec<u32> = Vec::new();
    if buf.try_reserve_exact(KEY_BUFSIZ).is_err() {
        return -1;
    }
    buf.resize(KEY_BUFSIZ, 0);
    el.el_keymacro.buf = buf;

    // Step 2.
    el.el_keymacro.map = None;

    // Step 3: a no-op, `map` having just been cleared. Kept because the C
    // calls it and because the call is what makes the two initialisation
    // paths identical.
    keymacro_reset(el);

    // Step 4. ERR-core-api-02: `el_init_internal` discards this -1 and hands
    // back an `EditLine` with no print buffer in it; the disposition there is
    // to fail construction, which is that function's business, not this one's.
    //
    // The C's comment claims this binds the arrow keys. It does not — see
    // ERR-input-43, dead and stale material.
    0
}

// [spec:libedit:def:keymacro.keymacro-end-fn]
// [spec:libedit:sem:keymacro.keymacro-end-fn]
/// Free the key maps.
pub(crate) fn keymacro_end(el: &mut EditLine) {
    // Step 1: `el_free(el->el_keymacro.buf)` then NULL. An empty `Vec` is
    // this port's NULL, and dropping the old one is the free.
    el.el_keymacro.buf = Vec::new();

    // Step 2. ERR-input-18 (fix): the C frees the tree with `node_free`,
    // which never looks at `type` and so leaks every `XK_STR` payload bound
    // during the `EditLine`'s life. The payloads are `Vec`s owned by the
    // nodes here, so they go with them; the leak is not reproduced, which the
    // rule says is unobservable across the C ABI.
    //
    // ERR-input-19 (fix): the C leaves `el_keymacro.map` pointing at freed
    // memory, so a second `keymacro_end` double-frees the whole trie. `.take()`
    // is the clearing the rule asks for — a second call now frees nothing.
    let map = el.el_keymacro.map.take();
    node_free(map);
}

// [spec:libedit:def:keymacro.keymacro-map-cmd-fn]
// [spec:libedit:sem:keymacro.keymacro-map-cmd-fn]
/// Package a command number as a [`KeymacroValueT`] for [`keymacro_add`] or
/// `terminal_set_arrow`.
///
/// The C returns `&el->el_keymacro.val`, a per-`EditLine` scratch slot whose
/// only purpose is to give that pointer an address; the idiom is always
/// build-and-consume in one expression. Rust cannot lend that slot out while
/// the consumer also takes `el`, so the value is returned by value.
pub(crate) fn keymacro_map_cmd(el: &mut EditLine, cmd: i32) -> KeymacroValueT {
    // Step 1: `(el_action_t)cmd`, an `unsigned char`, so anything outside
    // 0..255 is silently truncated. The scratch slot is still written, since
    // it is per-`EditLine` state the C leaves behind and the next
    // `keymacro_map_*` call overwrites.
    el.el_keymacro.val = KeymacroValueT::Cmd(cmd as ElActionT);

    // Step 2: the C's `&el->el_keymacro.val`.
    el.el_keymacro.val.clone()
}

// [spec:libedit:def:keymacro.keymacro-map-str-fn]
// [spec:libedit:sem:keymacro.keymacro-map-str-fn]
/// Package a macro expansion string as a [`KeymacroValueT`]. Returned by
/// value for the reason given on [`keymacro_map_cmd`]; note that
/// [`KeymacroValueT::Str`] owns its buffer, so this copies where the C
/// stored a pointer that dangled as soon as `map_bind` returned.
pub(crate) fn keymacro_map_str(el: &mut EditLine, str: &[u32]) -> KeymacroValueT {
    // Step 1. The C parks the caller's pointer in the shared slot without
    // copying, which is ERR-modes-14: `terminal_set_arrow` then keeps that
    // pointer in the function-key table long after `map_bind`'s stack frame
    // is gone. An owned copy cannot dangle, so the defect has nothing to
    // reproduce here — the value, not the address, is what survives.
    el.el_keymacro.val = KeymacroValueT::Str(upto_nul(str).to_vec());

    // Step 2.
    el.el_keymacro.val.clone()
}

// [spec:libedit:def:keymacro.keymacro-reset-fn]
// [spec:libedit:sem:keymacro.keymacro-reset-fn]
/// Drop the whole trie, leaving no bound sequences.
pub(crate) fn keymacro_reset(el: &mut EditLine) {
    // Steps 1 and 2 at once: the C frees through the field and then clears
    // it, which is one `take` here. Unlike `keymacro_end`'s `node_free`,
    // `node_put` releases the `XK_STR` payloads, so a reset does not leak in
    // the C either.
    //
    // The action tables are deliberately untouched: any `ED_SEQUENCE_LEAD_IN`
    // entry in `el_map.key`/`alt` now points at nothing, and both callers
    // overwrite the tables immediately afterwards.
    let map = el.el_keymacro.map.take();
    node_put(map);
}

// [spec:libedit:def:keymacro.keymacro-get-fn]
// [spec:libedit:sem:keymacro.keymacro-get-fn]
/// Read characters until a key sequence matches or mismatches. Returns the
/// `XK_` type of the match; the last character read is left in `ch`.
pub(crate) fn keymacro_get(el: &mut EditLine, ch: &mut u32, val: &mut KeymacroValueT) -> i32 {
    // The C passes `el->el_keymacro.map` and `el` to the same call. The trie
    // has to come out of the `EditLine` for that, because `node_trav` reads
    // more input through `el`; it goes straight back afterwards. Nothing
    // reachable from `el_wgetc` looks at the trie, so the window in which the
    // field reads empty is not observable.
    let map = el.el_keymacro.map.take();
    let ret = node_trav(el, map.as_deref(), ch, val);
    el.el_keymacro.map = map;
    ret
}

// [spec:libedit:def:keymacro.keymacro-add-fn]
// [spec:libedit:sem:keymacro.keymacro-add-fn]
/// Bind `key` to `val`. `val` is borrowed and copied into the trie; it is
/// not always the scratch slot — `terminal.c` passes `&arrow[i].fun`.
pub(crate) fn keymacro_add(el: &mut EditLine, key: &[u32], val: &KeymacroValueT, ntype: i32) {
    let key = upto_nul(key);

    // Step 1.
    if key.is_empty() {
        el.write_errfile(b"keymacro_add: Null extended-key not allowed.\n");
        return;
    }

    // Step 2. The guard is only applied to `XK_CMD`; there is no equivalent
    // check on anything else. A `Str` value with `ntype == XK_CMD` is the C
    // reading a command byte out of a pointer, which has no counterpart here
    // and no caller.
    if ntype == XK_CMD && matches!(val, KeymacroValueT::Cmd(cmd) if *cmd == ED_SEQUENCE_LEAD_IN) {
        el.write_errfile(b"keymacro_add: sequence-lead-in command not allowed\n");
        return;
    }

    // Step 3. ERR-input-04 (define — propagate the failure): the C does not
    // check `node_get`, so an out-of-memory here becomes a NULL dereference
    // inside `node_try`. Defined as "the trie stays empty and nothing is
    // bound", which is also what `node_try` reports through the -1 that
    // `keymacro_add` has always dropped.
    if el.el_keymacro.map.is_none() {
        el.el_keymacro.map = node_get(key[0]);
    }

    // Step 4: the trie comes out of the `EditLine` because `node_try` needs
    // both it and `el` — see `keymacro_get` for the same move. The return
    // value is discarded, so out-of-memory on the `wcsdup` of an `XK_STR`
    // value is silently swallowed exactly as in the C (ERR-input-31).
    let mut map = el.el_keymacro.map.take();
    if let Some(root) = map.as_deref_mut() {
        node_try(root, key, val, ntype);
    }
    el.el_keymacro.map = map;
}

// [spec:libedit:def:keymacro.keymacro-clear-fn]
// [spec:libedit:sem:keymacro.keymacro-clear-fn]
/// Drop a sequence binding whose lead-in is being rebound. `map` is the C's
/// `el_action_t *map`, always `el_map.key` or `el_map.alt`, so it is the
/// selector rather than a second alias of `el`.
pub(crate) fn keymacro_clear(el: &mut EditLine, map: ElMapCurrent, r#in: &[u32]) {
    let r#in = upto_nul(r#in);

    // The C reads `*in` unconditionally; for an empty string that is the
    // terminator, so slot 0 of the tables decides. Same here.
    let c = r#in.first().copied().unwrap_or(0);

    // Step 1, with ERR-input-29 (fix): the C's guard is `*in > N_KEYS`, one
    // too permissive for a 256-entry table, so a `*in` of exactly 256 — and,
    // where `wchar_t` is signed, any negative one — passes and the
    // `(unsigned char)` cast wraps it into range, taking the decision from
    // the wrong slot. The rule directs the port to test the whole code point
    // against `N_KEYS`; `u32` also removes the signed half of the defect.
    if c >= N_KEYS as u32 {
        return;
    }
    let c = c as usize;

    // Step 2. The C's `map[(unsigned char)*in]` against pointer identity with
    // `el_map.key`/`el_map.alt`; the selector says which. `map` being neither
    // cannot arise, so the C's "does nothing at all" third case is not
    // expressible.
    //
    // `get` rather than an index: ERR-core-api-02 leaves both tables empty
    // when `map_init` failed, where the C dereferences NULL. Defined as "not
    // a lead-in", so nothing is deleted.
    let key_c = el.el_map.key.get(c).copied().unwrap_or(0);
    let alt_c = el.el_map.alt.get(c).copied().unwrap_or(0);
    let hit = match map {
        ElMapCurrent::Key => key_c == ED_SEQUENCE_LEAD_IN && alt_c != ED_SEQUENCE_LEAD_IN,
        ElMapCurrent::Alt => alt_c == ED_SEQUENCE_LEAD_IN && key_c != ED_SEQUENCE_LEAD_IN,
    };

    // Step 3: the whole string, not just its first character, and the result
    // is discarded. Step 4: neither table is touched here.
    if hit {
        keymacro_delete(el, r#in);
    }
}

// [spec:libedit:def:keymacro.keymacro-delete-fn]
// [spec:libedit:sem:keymacro.keymacro-delete-fn]
/// Delete `key` and every longer key starting with it.
pub(crate) fn keymacro_delete(el: &mut EditLine, key: &[u32]) -> i32 {
    let key = upto_nul(key);

    // Step 1: the only -1 this function has.
    if key.is_empty() {
        el.write_errfile(b"keymacro_delete: Null extended-key not allowed.\n");
        return -1;
    }

    // Step 2.
    if el.el_keymacro.map.is_none() {
        return 0;
    }

    // Step 3: the C passes `&el->el_keymacro.map` so the root slot itself can
    // be rewritten, NULL included. Taking the field out and putting it back is
    // that same slot, reached the way the borrow checker allows.
    //
    // The "was anything deleted?" answer is discarded, so no caller can tell.
    // The action tables keep their `ED_SEQUENCE_LEAD_IN` entries; callers fix
    // that up, and when they do not, `keymacro_get` meets a trie that no
    // longer has the sequence in it (ERR-input-03).
    let mut map = el.el_keymacro.map.take();
    node_delete(&mut map, key);
    el.el_keymacro.map = map;

    // Step 4.
    0
}

// [spec:libedit:def:keymacro.keymacro-print-fn]
// [spec:libedit:sem:keymacro.keymacro-print-fn]
/// Print the binding for `key`, or the whole trie for an empty key.
pub(crate) fn keymacro_print(el: &mut EditLine, key: &[u32]) {
    let key = upto_nul(key);

    // Step 1.
    if el.el_keymacro.map.is_none() && key.is_empty() {
        return;
    }

    // Step 2. ERR-core-api-02: with no print buffer — `keymacro_init`'s
    // allocation having failed — the C writes through NULL here. Defined as
    // printing nothing at all: there is nowhere to assemble the key text, and
    // the "Unbound extended key" message below would misreport a binding that
    // may well exist.
    if el.el_keymacro.buf.is_empty() {
        return;
    }
    el.el_keymacro.buf[0] = u32::from(b'"');

    // Step 3: the `1` is the opening quote already in the buffer. The trie
    // comes out of the `EditLine` for the duration, as in `keymacro_get`.
    let map = el.el_keymacro.map.take();
    let found = node_lookup(el, Some(key), map.as_deref(), 1);
    el.el_keymacro.map = map;

    // Step 4. `node_lookup` returns only 0 or -1, so this is exactly its
    // "not bound / did not fit" answer. Note step 2 has already written the
    // buffer even on this path.
    if found <= -1 {
        let mut msg = b"Unbound extended key \"".to_vec();
        msg.extend_from_slice(&encode_wide(key));
        msg.extend_from_slice(b"\"\n");
        el.write_errfile(&msg);
    }
}

// [spec:libedit:def:keymacro.node-trav-fn]
// [spec:libedit:sem:keymacro.node-trav-fn]
/// Walk the trie from `ptr` until a match or a mismatch, reading more
/// characters as needed. `ptr` is `Option` because the C dereferences
/// `el->el_keymacro.map` here without checking it.
fn node_trav(
    el: &mut EditLine,
    ptr: Option<&KeymacroNodeT>,
    ch: &mut u32,
    val: &mut KeymacroValueT,
) -> i32 {
    // ERR-input-03 (define — return "no match" for an empty trie). The C
    // dereferences its node pointer with no NULL check and relies on the
    // invariant that a character only reaches the trie when its action table
    // entry is `ED_SEQUENCE_LEAD_IN` — an invariant `map_bind`'s multi-character
    // `-r` path can break. The empty trie answers the way a dead end does.
    let Some(ptr) = ptr else {
        *val = KeymacroValueT::Str(Vec::new());
        return XK_STR;
    };

    // Step 1.
    if ptr.ch == *ch {
        if let Some(next) = ptr.next.as_deref() {
            // Step 1a: the key is not complete, so block for another
            // character. `el_wgetc` drains pushed macro text first and only
            // then reads the tty, which is why there is no "partial match"
            // return code. Anything but 1 — end of file, read error, or a tty
            // that could not be put in raw mode — loses every character
            // consumed so far.
            if el_wgetc(el, ch) != 1 {
                return XK_NOD;
            }
            // Only `next` is followed; the child level's sibling chain is the
            // recursive call's own job.
            node_trav(el, Some(next), ch, val)
        } else {
            // Step 1b: a leaf, so the sequence is complete. The C's `*val =
            // ptr->val` hands out a pointer that aliases the node's own
            // string, which the caller must not free and must not hold across
            // a rebind; the clone here owns its copy, so that hazard has no
            // counterpart. The protocol is unchanged.
            *val = ptr.val.clone();
            if ptr.r#type != XK_CMD {
                // Command bindings want the character that invoked them;
                // macro expansions do not.
                *ch = 0;
            }
            ptr.r#type
        }
    } else if let Some(sibling) = ptr.sibling.as_deref() {
        // Step 2a: same `*ch`, next sibling.
        node_trav(el, Some(sibling), ch, val)
    } else {
        // Step 2b: a dead end. ERR-input-33 (reproduce): "no match" is
        // `XK_STR` with a NULL string, the same code a real macro binding
        // uses, so the caller must test the payload; and every character
        // consumed on the way in is dropped with no pushback. The C's NULL
        // `str` is the empty buffer here — the representation `def` chose.
        *val = KeymacroValueT::Str(Vec::new());
        XK_STR
    }
}

// [spec:libedit:def:keymacro.node-try-fn]
// [spec:libedit:sem:keymacro.node-try-fn]
/// Find the node matching `str` or allocate one, then store `val` there.
fn node_try(ptr: &mut KeymacroNodeT, str: &[u32], val: &KeymacroValueT, ntype: i32) -> i32 {
    // The C reads `*str` unguarded. It is never the terminator: `keymacro_add`
    // rejects the empty key and step 3 only recurses with characters left.
    let Some(&c) = str.first() else {
        return 0;
    };

    // Step 1: locate or create the node for `*str` at this level.
    let mut node: &mut KeymacroNodeT = ptr;
    if node.ch != c {
        // Step 1a: walk to the matching sibling, or to the end of the chain.
        while node.sibling.as_deref().is_some_and(|s| s.ch != c) {
            node = node.sibling.as_deref_mut().unwrap();
        }
        if node.sibling.is_none() {
            // Step 1b: new siblings go on the END of the chain, so chains are
            // in insertion order — which is the order `node_enum` prints them.
            node.sibling = node_get(c);
            if node.sibling.is_none() {
                // ERR-input-04 (define — propagate the failure). The C stores
                // the NULL and dereferences it on the next line.
                return -1;
            }
        }
        // Step 1c.
        node = node.sibling.as_deref_mut().unwrap();
    }

    // Step 2: `*++str`.
    let str = &str[1..];
    if str.is_empty() {
        // Step 2a. ERR-input-32 (reproduce): this is the destructive
        // direction of the shadowing rule. Binding a key that is a proper
        // prefix of existing longer keys frees the entire child level, all its
        // siblings, all their subtrees and every payload in them — with
        // `"abcd"` and `"abcef"` bound, adding `"abc"` loses both. The file
        // header warns about it.
        if node.next.is_some() {
            let next = node.next.take();
            node_put(next);
        }

        // Step 2b: release the old payload, switching on the CURRENT type.
        // `XK_CMD` and `XK_NOD` need nothing; an `XK_STR` payload is dropped
        // by the assignment in step 2c, so the arm is empty here too. The
        // abort is unreachable — a node only acquires a type through step 2c,
        // which aborts on anything else first — and is kept for its shape.
        match node.r#type {
            XK_CMD | XK_NOD | XK_STR => {}
            _ => process::abort(),
        }

        // Step 2c: install the new payload.
        //
        // ERR-input-31 (fix): the C writes `ptr->type = ntype` *before* the
        // switch, so a failed `wcsdup` leaves the node `XK_STR` with a NULL
        // string — a state indistinguishable at lookup time from `node_trav`'s
        // no-match answer — and the abort path has already mutated the node.
        // The rule's fix is to propagate the failure rather than leave a
        // poisoned node, so the copy is made first and both fields are
        // committed together. The -1 still cannot escape: step 3 discards the
        // recursive result and `keymacro_add` discards the outermost one.
        match ntype {
            XK_CMD => {
                // C: `ptr->val = *val`, the union copied by value.
                node.r#type = ntype;
                node.val = val.clone();
            }
            XK_STR => {
                // C: `ptr->val.str = wcsdup(val->str)`. A `Cmd` value with
                // `ntype == XK_STR` would be the C calling `wcsdup` on a
                // command byte; no caller does it, and it is defined here as
                // the empty expansion.
                let src: &[u32] = match val {
                    // `wcsdup` stops at the terminator; a value carrying one
                    // in the middle — `terminal.c` builds its own out of a
                    // fixed-width capability buffer — is copied only that far.
                    KeymacroValueT::Str(s) => upto_nul(s),
                    KeymacroValueT::Cmd(_) => &[],
                };
                let mut copy: Vec<u32> = Vec::new();
                if copy.try_reserve_exact(src.len()).is_err() {
                    return -1;
                }
                copy.extend_from_slice(src);
                node.r#type = ntype;
                node.val = KeymacroValueT::Str(copy);
            }
            _ => {
                // ERR-input-30 (reproduce). `EL_ABORT` is `abort(3)` in a
                // non-DEBUG build, and this is reachable:
                // `terminal_reset_arrow` passes `arrow[i].type` straight
                // through and `terminal_clear_arrow` (`bind -k -r up`) sets
                // that field to `XK_NOD`, so a later `bind -e`/`bind -v`
                // kills the process. Defined C behaviour, so the conformance
                // policy's default applies: reproduce.
                process::abort();
            }
        }
    } else {
        // Step 3: more characters to place.
        if node.next.is_none() {
            node.next = node_get(str[0]);
            if node.next.is_none() {
                // ERR-input-04 again.
                return -1;
            }
        }
        // ERR-input-32 (reproduce), the other direction of the shadowing
        // rule: this branch does not touch `type` or `val`, so a node that
        // already held a complete binding keeps it and merely gains a child.
        // The shorter binding stays allocated but becomes unreachable, because
        // `node_trav` tests `next` before it looks at `type` — and if the
        // longer key is deleted later, `node_delete`'s prune frees the
        // shadowed binding rather than restoring it.
        let next = node.next.as_deref_mut().unwrap();
        node_try(next, str, val, ntype);
    }

    // Step 4.
    0
}

// [spec:libedit:def:keymacro.node-delete-fn]
// [spec:libedit:sem:keymacro.node-delete-fn]
/// Delete the node matching `str`. `inptr` is the C's `keymacro_node_t **`:
/// the link slot itself, so the node can be unlinked and dropped.
fn node_delete(inptr: &mut Option<Box<KeymacroNodeT>>, str: &[u32]) -> i32 {
    // As in `node_try`, the C reads `*str` unguarded and never reaches here
    // with an exhausted key.
    let Some(&c) = str.first() else {
        return 0;
    };

    // Steps 1 and 2 together. The C keeps `ptr` and `prev_ptr` and unlinks
    // through `*inptr` when `prev_ptr` is NULL; the equivalent is to carry the
    // link slot that points at the candidate, which starts as `inptr` itself
    // — that is exactly what "`prev_ptr` stays NULL when the head matched"
    // means. `*inptr` is never NULL-checked by the C; running off the end of
    // the chain is its "no node matches this character" answer, and an empty
    // level answers the same way.
    let mut slot: &mut Option<Box<KeymacroNodeT>> = inptr;
    loop {
        match slot.as_deref().map(|n| n.ch) {
            None => return 0,
            Some(ch) if ch == c => break,
            Some(_) => slot = &mut slot.as_deref_mut().unwrap().sibling,
        }
    }

    // Step 3: `*++str`.
    let str = &str[1..];
    if str.is_empty() {
        // Steps 3a to 3d. Taking the node out of its slot and putting its
        // sibling back is the C's unlink; the C's separate `ptr->sibling =
        // NULL` — load-bearing, because `node_put` follows sibling links and
        // would otherwise take the rest of the relinked chain with it — is the
        // `take` that moved the sibling out.
        //
        // Deleting a key deletes every longer key that has it as a prefix,
        // since they all hang off this node's `next`.
        let mut victim = slot.take().unwrap();
        *slot = victim.sibling.take();
        node_put(Some(victim));
        return 1;
    }

    // Step 4: descend, and prune on the way back out.
    let node = slot.as_deref_mut().unwrap();
    if node.next.is_some() && node_delete(&mut node.next, str) == 1 {
        // The child level had other siblings and the recursion re-pointed the
        // slot at one of them, so this node is still needed. Returning 0 is
        // what stops the prune propagating further up.
        if slot.as_deref().unwrap().next.is_some() {
            return 0;
        }
        // ERR-input-32 (reproduce): the prune never looks at `type`. A node
        // that carried a complete binding of its own *and* had children — the
        // shadowing `node_try` step 3 creates — is freed here along with its
        // binding. Binding `"ab"`, then `"abc"`, then deleting `"abc"` leaves
        // neither bound, and the prune runs back to the root so the `"a"` node
        // goes too.
        let mut victim = slot.take().unwrap();
        *slot = victim.sibling.take();
        node_put(Some(victim));
        return 1;
    }

    // Step 5.
    0
}

// [spec:libedit:def:keymacro.node-put-fn]
// [spec:libedit:sem:keymacro.node-put-fn]
/// Free a whole subtree. Takes the node by value: the C's `el_free` chain is
/// a drop here.
///
/// The C also passes `el`, but only a disabled `DEBUG` diagnostic reads it;
/// the Rust helper therefore carries only the tree it owns.
fn node_put(ptr: Option<Box<KeymacroNodeT>>) {
    // Step 1. Despite the name and the C's comment there is no free list and
    // no reuse: this is a drop, not a pool return.
    let Some(mut ptr) = ptr else {
        return;
    };

    // Step 2: children first. The C's dead `ptr->next = NULL` after the
    // recursive call is the `take` that fed it.
    let next = ptr.next.take();
    node_put(next);

    // Step 3: then the rest of the sibling chain. The C does not NULL
    // `sibling`, which callers depend on in both directions — `node_try`
    // drops a whole child level with one call, and `node_delete` therefore
    // has to unlink its victim before calling. Taking it here keeps the same
    // reachability while making the drop below non-recursive.
    let sibling = ptr.sibling.take();
    node_put(sibling);

    // Step 4: this node's payload, by `type`. `XK_CMD` and `XK_NOD` have
    // nothing to free and an `XK_STR` payload is owned by the node, so it goes
    // with it in step 5. The abort is the C's `EL_ABORT((el->el_errfile, "Bad
    // XK_ type %d\n", ptr->type))` — `abort(3)` in a non-DEBUG build — and is
    // unreachable: `node_try` is the only writer of `type` and aborts on a
    // bad one first (ERR-input-30).
    match ptr.r#type {
        XK_CMD | XK_NOD | XK_STR => {}
        _ => process::abort(),
    }

    // Step 5. This is the only path that releases `XK_STR` payloads in the C;
    // `node_free` does not (ERR-input-18).
    drop(ptr);
}

// [spec:libedit:def:keymacro.node-get-fn]
// [spec:libedit:sem:keymacro.node-get-fn]
/// Allocate one unlinked `XK_NOD` node for `ch`. `Option` keeps the C's
/// allocation-failure return, which none of its callers check.
fn node_get(ch: u32) -> Option<Box<KeymacroNodeT>> {
    // Steps 1 to 6. `Box::new` has no fallible form on stable, so the C's
    // NULL return is unreachable from here — Rust's allocator aborts first.
    // The `Option` stays because it is the contract the rule states and
    // because the call sites now handle it (ERR-input-04), which is where the
    // C's real defect lives.
    //
    // `type` is `XK_NOD`: the node exists only because it lies on the path of
    // some longer key, and `node_try` overwrites it if and when the node
    // becomes the last character of a bound sequence. The C's `val.str = NULL`
    // is the empty `Str` — the same state `node_trav` reports as "no match".
    Some(Box::new(KeymacroNodeT {
        ch,
        r#type: XK_NOD,
        val: KeymacroValueT::Str(Vec::new()),
        next: None,
        sibling: None,
    }))
}

// [spec:libedit:def:keymacro.node-free-fn]
// [spec:libedit:sem:keymacro.node-free-fn]
/// Free a node and its `next`/`sibling` chains, without touching the macro
/// strings — the leak `sem:keymacro.node-free-fn` records.
fn node_free(k: Option<Box<KeymacroNodeT>>) {
    // Step 1.
    let Some(mut k) = k else {
        return;
    };

    // Steps 2 and 3: siblings first, then children — the C's order. It only
    // matters in that both links must be followed before the node itself goes.
    node_free(k.sibling.take());
    node_free(k.next.take());

    // Step 4. ERR-input-18 (fix): the C reads no `type` here and so frees no
    // `val.str`, leaking every macro string at teardown. The payload is owned
    // by the node in this port, so it cannot be left behind; the leak is not
    // reproduced. What survives is the other difference from `node_put` —
    // this one never aborts on an unrecognised `type`, because it never looks.
    drop(k);
}

// [spec:libedit:def:keymacro.node-lookup-fn]
// [spec:libedit:sem:keymacro.node-lookup-fn]
/// Look for `str` from node `ptr`, printing the binding at the leaf. `str`
/// is `Option` because the C tests `!str`.
fn node_lookup(
    el: &mut EditLine,
    str: Option<&[u32]>,
    ptr: Option<&KeymacroNodeT>,
    cnt: usize,
) -> i32 {
    // Step 1: "cannot have null ptr" — the empty-map case as well as the
    // recursion's guard.
    let Some(ptr) = ptr else {
        return -1;
    };

    // Step 2: no key characters left to match, so enumerate everything at and
    // below `ptr` including its whole sibling chain. This is how an empty key
    // dumps the map and how a prefix key lists its completions.
    let str = match str {
        None => {
            node_enum(el, Some(ptr), cnt);
            return 0;
        }
        Some(s) if s.is_empty() || s[0] == 0 => {
            node_enum(el, Some(ptr), cnt);
            return 0;
        }
        Some(s) => s,
    };

    // Step 3: a match at this position.
    if ptr.ch == str[0] {
        // Step 3a. The C passes `KEY_BUFSIZ - cnt`; the slice carries the
        // same bound, and a `cnt` past the end of the buffer — which the
        // arithmetic below cannot produce — reads as "no room left".
        let used = match el.el_keymacro.buf.get_mut(cnt..) {
            Some(dst) => ct_visual_char(dst, ptr.ch),
            None => -1,
        };
        if used == -1 {
            return -1; // ran out of buffer space
        }
        let used = used as usize;

        if let Some(next) = ptr.next.as_deref() {
            // Step 3b. If `str + 1` is the terminator the recursion takes
            // step 2 and enumerates the whole subtree — the prefix case.
            node_lookup(el, Some(&str[1..]), Some(next), cnt + used)
        } else if str.len() == 1 {
            // Step 3c: a leaf and the key is complete. "Did not fit" is the
            // answer step 3a already has.
            if !kprint_leaf(el, cnt + used, &ptr.val, ptr.r#type) {
                return -1;
            }
            0
        } else {
            // The caller's key is longer than any binding on this path.
            -1
        }
    } else if let Some(sibling) = ptr.sibling.as_deref() {
        // Step 4: no match, so try the sibling with the SAME `str` and `cnt`.
        node_lookup(el, Some(str), Some(sibling), cnt)
    } else {
        -1
    }
}

// [spec:libedit:def:keymacro.node-enum-fn]
// [spec:libedit:sem:keymacro.node-enum-fn]
/// Print every binding at or below `ptr`, accumulating the key into
/// `el_keymacro.buf` at offset `cnt`.
fn node_enum(el: &mut EditLine, ptr: Option<&KeymacroNodeT>, cnt: usize) -> i32 {
    // Step 1: the buffer-exhaustion guard, before anything else.
    if cnt >= KEY_BUFSIZ - 5 {
        // ERR-input-34 (fix): the C's two writes are PRE-increments, so the
        // quote lands at `cnt + 1` and the terminator at `cnt + 2`, leaving
        // `buf[cnt]` holding a stale character from a previously printed key.
        // The rule says the intent was plainly `buf[cnt]` and `buf[cnt + 1]`.
        //
        // The bound check is this function's share of ERR-input-05: `cnt` can
        // reach `KEY_BUFSIZ` through step 3, and the C writes past the end
        // there too.
        if cnt + 1 < el.el_keymacro.buf.len() {
            el.el_keymacro.buf[cnt] = u32::from(b'"');
            el.el_keymacro.buf[cnt + 1] = 0;
        }
        el.write_errfile(b"Some extended keys too long for internal print buffer");
        let mut msg = b" \"".to_vec();
        msg.extend_from_slice(&encode_wide(&el.el_keymacro.buf));
        msg.extend_from_slice(b"...\"\n");
        el.write_errfile(&msg);
        return 0;
    }

    // Step 2: only a bad caller gets here; the recursion never passes NULL.
    // The C's `node_enum: BUG!! Null ptr passed` is a `DEBUG_EDIT` build only.
    let Some(ptr) = ptr else {
        return -1;
    };

    // Step 3: append this node's character.
    let used = match el.el_keymacro.buf.get_mut(cnt..) {
        Some(dst) => ct_visual_char(dst, ptr.ch),
        None => -1,
    };
    // ERR-input-06 (define — check for -1 and bail out the way `node_lookup`
    // does). Step 1 only reserves six free `wchar_t` while a non-BMP
    // non-printable needs eight, so the C's unchecked `cnt + (size_t)used`
    // becomes `cnt - 1` and the writes below land over the previous character.
    if used == -1 {
        return -1;
    }
    let used = used as usize;

    if ptr.next.is_none() {
        // Step 4: a complete binding. The "is this a binding" test is
        // `next == NULL` and NOT `type != XK_NOD`, so an interior node that
        // also carries a complete binding — a key shadowed by a longer one —
        // is never printed, exactly as it is never returned by `node_trav`.
        if !kprint_leaf(el, cnt + used, &ptr.val, ptr.r#type) {
            return -1;
        }
    } else {
        node_enum(el, ptr.next.as_deref(), cnt + used);
    }

    // Step 5: siblings with the ORIGINAL `cnt`, so a sibling's rendering
    // overwrites this node's character in the buffer. Depth-first, children
    // before siblings.
    if let Some(sibling) = ptr.sibling.as_deref() {
        node_enum(el, Some(sibling), cnt);
    }

    // Step 6.
    0
}

// [spec:libedit:def:keymacro.keymacro-kprint-fn]
// [spec:libedit:sem:keymacro.keymacro-kprint-fn]
/// Print `key` and the function or string `val` binds it to. `val` is
/// `Option` because the C prints "no input" for a NULL one.
pub(crate) fn keymacro_kprint(
    el: &mut EditLine,
    key: &[u32],
    val: Option<&KeymacroValueT>,
    ntype: i32,
) {
    // Step 1: `ntype` is not consulted.
    let Some(val) = val else {
        let key = encode_key(el, key);
        kprint_line(el, &key, b"no input");
        return;
    };

    // Step 2.
    match ntype {
        XK_STR => {
            // The C writes the separator as `ntype == XK_STR ? "\"\"" : "[]"`
            // *inside* the `XK_STR` case, so the condition is always true and
            // the `"[]"` arm is dead — a leftover from a removed `XK_EXE`
            // type (ERR-input-43). Implemented as the constant.
            //
            // A `Cmd` value with `ntype == XK_STR` would be the C rendering a
            // command byte as a string pointer; defined here as the empty
            // expansion, which `keymacro_decode_str` renders `^@`.
            let str: &[u32] = match val {
                KeymacroValueT::Str(s) => s,
                KeymacroValueT::Cmd(_) => &[],
            };
            let mut unparsbuf = [0u8; EL_BUFSIZ];
            keymacro_decode_str(str, &mut unparsbuf, EL_BUFSIZ, b"\"\"");
            let what = upto_nul(&unparsbuf).to_vec();
            let key = encode_key(el, key);
            kprint_line(el, &key, &what);
        }
        XK_CMD => {
            // A `Str` value with `ntype == XK_CMD` is the C reading the
            // union's `cmd` out of a pointer's low byte. Defined here as a
            // command number no help entry can carry, so nothing is printed —
            // the same outcome the C's scan has for an unmatched command.
            let cmd = match val {
                KeymacroValueT::Cmd(c) => i32::from(*c),
                KeymacroValueT::Str(_) => -1,
            };

            // ERR-input-07 (define — bound the scan by `el_map.nfunc`). The C
            // terminates on a NULL `name` sentinel that the generated help
            // table does not contain, so an unmatched command walks off the
            // end of the allocation. `nfunc` and `help.len()` are allowed to
            // disagree on `map_addfunc`'s failure path, hence `take`.
            //
            // On no match NOTHING is printed at all; only a `DEBUG_KEY` build
            // says so, and that report is itself the out-of-bounds read.
            let name = el
                .el_map
                .help
                .iter()
                .take(el.el_map.nfunc)
                .find(|fp| cmd == fp.func)
                .map(|fp| fp.name.to_vec());

            if let Some(name) = name {
                // C: `wcstombs(unparsbuf, fp->name, sizeof(unparsbuf))` with
                // its return value unchecked, then a forced terminator.
                let mut unparsbuf = [0u8; EL_BUFSIZ];
                let used = wcstombs_into(&mut unparsbuf, &name);
                let key = encode_key(el, key);
                kprint_line(el, &key, &unparsbuf[..used]);
            }
        }
        _ => {
            // ERR-input-30 (reproduce), as in `node_try`: `EL_ABORT` is
            // `abort(3)`. `XK_NOD` reaches here too — `terminal_print_arrow`
            // is the C's live path to it.
            process::abort();
        }
    }
}

// [spec:libedit:def:keymacro.keymacro-decode-str-fn]
// [spec:libedit:sem:keymacro.keymacro-decode-str-fn]
/// Make a printable, `sep`-wrapped narrow version of `str` in `buf`,
/// returning the length it wanted — which may exceed `len`. `len` is kept
/// alongside the slice because the rule indexes by it.
pub(crate) fn keymacro_decode_str(str: &[u32], buf: &mut [u8], len: usize, sep: &[u8]) -> usize {
    // ERR-input-09 (define — reject `len == 0` explicitly). The C's `eb ==
    // buf == b` then lets step 1 push `b` past `eb`, `(size_t)(eb - b)` wrap
    // to `SIZE_MAX`, and the forced termination write `buf[-1]`.
    //
    // The C also trusts `len` to describe `buf`; clamping is the same
    // definition applied to the other direction of that mismatch.
    let len = len.min(buf.len());
    if len == 0 {
        return 0;
    }

    // The C's `sep` is a NUL-terminated string, read at most two characters
    // deep. A slice ends where it ends.
    let sep0 = sep.first().copied().unwrap_or(0);
    let sep1 = sep.get(1).copied().unwrap_or(0);

    // The write cursor. `addc` is the C's `ADDC`: it writes only while
    // `b < eb` but keeps counting either way, which is what makes the return
    // value under-report on truncation (ERR-input-35, reproduce).
    let mut b = 0usize;

    // Step 1.
    if sep0 != 0 {
        addc(buf, len, &mut b, sep0);
    }

    let str = upto_nul(str);
    if str.is_empty() {
        // Step 2: the empty sequence renders as `^@`, and the loop is skipped.
        addc(buf, len, &mut b, b'^');
        addc(buf, len, &mut b, b'@');
    } else {
        // Step 3.
        'encode: for &p in str {
            // Step 3a. 8 wide characters is always enough for
            // `ct_visual_char`, so this never fails; a -1 would leave the
            // inner loop empty anyway, as the C's `while (l-- > 0)` does.
            let mut dbuf = [0u32; VISUAL_WIDTH_MAX];
            let l = ct_visual_char(&mut dbuf, p);
            let l = usize::try_from(l).unwrap_or(0);

            // Step 3b.
            for &c in &dbuf[..l] {
                // C: `ct_encode_char(b, (size_t)(eb - b), *p2++)`. `b` never
                // passes `eb` inside this loop, so the slice always exists;
                // the `else` is the C's wrapped length, now unreachable.
                let Some(dst) = buf.get_mut(b..len) else {
                    break 'encode;
                };
                let n = ct_encode_char(dst, c);
                if n == -1 {
                    // Out of room. The output is truncated on a whole
                    // character boundary, never mid-sequence.
                    break 'encode;
                }
                b += n as usize;
            }
        }
    }

    // Step 4: a one-character `sep` opens without closing.
    if sep0 != 0 && sep1 != 0 {
        addc(buf, len, &mut b, sep1);
    }

    // Step 5.
    addc(buf, len, &mut b, 0);

    // Step 6: always NUL-terminated when `len > 0`.
    if b >= len {
        buf[len - 1] = 0;
    }

    // Step 7. Not the length the full rendering would have needed: it counts
    // the bytes the loop actually wrote plus the separator and NUL bytes
    // whether or not those were written. Every in-tree caller discards it.
    b
}

/// The tail [`node_lookup`] and [`node_enum`] share: close the assembled key
/// text at `px` with the C's second quote, terminate it, and print what the
/// leaf binds. `false` is "it did not fit", which both callers report as -1.
///
/// ERR-input-05 (define — bounds-check before writing) lives here rather than
/// in both. The C writes both `wchar_t` with no check at all, and neither
/// caller's arithmetic leaves room for them: `ct_visual_char` only guarantees
/// `cnt + used <= KEY_BUFSIZ`, and `node_enum`'s entry guard reserves six
/// `wchar_t` where a non-BMP non-printable takes eight. Two copies of the
/// check are two chances for one of them to drift off the write it guards.
///
/// The C hands `keymacro_kprint` the shared buffer itself. Copying the
/// assembled text out is the same string — `kprint` reads to the terminator
/// and never writes — and it is what lets `el` be passed mutably.
fn kprint_leaf(el: &mut EditLine, px: usize, val: &KeymacroValueT, ntype: i32) -> bool {
    if px + 1 >= el.el_keymacro.buf.len() {
        return false;
    }
    el.el_keymacro.buf[px] = u32::from(b'"');
    el.el_keymacro.buf[px + 1] = 0;
    let key: Vec<u32> = el.el_keymacro.buf[..=px].to_vec();
    keymacro_kprint(el, &key, Some(val), ntype);
    true
}

// ---------------------------------------------------------------------------
// Host facilities the C gets from libc. None of these is a ported function.
// ---------------------------------------------------------------------------

/// C: the `ADDC(c)` macro — write while there is room, count regardless.
fn addc(buf: &mut [u8], len: usize, b: &mut usize, c: u8) {
    if *b < len {
        buf[*b] = c;
    }
    *b += 1;
}

/// C: `%ls` — the wide string rendered through the locale's multibyte
/// encoding, stopping at the first character it cannot represent.
fn encode_wide(s: &[u32]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut scratch = [0u8; crate::compat::locale::MB_LEN_MAX];
    for &c in upto_nul(s) {
        let n = ct_encode_char(&mut scratch, c);
        if n <= 0 {
            break;
        }
        out.extend_from_slice(&scratch[..n as usize]);
    }
    out
}

/// C: `wcstombs(dst, src, len)` followed by `dst[len - 1] = '\0'`. Returns
/// the byte count of the converted prefix.
fn wcstombs_into(dst: &mut [u8], src: &[u32]) -> usize {
    let mut used = 0usize;
    for &c in upto_nul(src) {
        // -1 is "no room left"; 0 is a character this locale cannot represent,
        // where `wcstombs` fails outright and leaves the buffer's contents
        // unspecified apart from the forced terminator (ERR-encoding-15).
        let n = ct_encode_char(&mut dst[used..], c);
        if n <= 0 {
            break;
        }
        used += n as usize;
    }
    // The forced terminator truncates a conversion that exactly filled the
    // buffer.
    used.min(dst.len().saturating_sub(1))
}

/// C: `ct_encode_string(key, &el->el_scratch)` handed straight to `%s`.
///
/// ERR-input-08 (define): that is NULL on allocation failure, and glibc
/// prints `(null)` for it. The C's observable result on our targets is what
/// the port defines.
fn encode_key(el: &mut EditLine, key: &[u32]) -> Vec<u8> {
    ct_encode_string(Some(key), &mut el.el_scratch)
        .map_or_else(|| b"(null)".to_vec(), <[u8]>::to_vec)
}

/// C: `fprintf(el->el_outfile, "%-15s->  %s\n", key, what)`.
fn kprint_line(el: &EditLine, key: &[u8], what: &[u8]) {
    let mut out = key.to_vec();
    // `%-15s` pads to 15 *bytes*, not characters.
    if out.len() < 15 {
        out.resize(15, b' ');
    }
    out.extend_from_slice(b"->  ");
    out.extend_from_slice(what);
    out.push(b'\n');
    el.write_outfile(&out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compat::el::blank_editline;
    use crate::compat::read::{el_wpush, read_init};

    fn w(s: &str) -> Vec<u32> {
        s.chars().map(u32::from).collect()
    }

    /// An editor with the read subsystem up and no streams. The three
    /// descriptors a `calloc`ed `EditLine` carries are 0 — the process's
    /// standard input — and the diagnostics below would land there; -1 is this
    /// crate's "no stream". `el_infd` at -1 also makes the tty unreadable, so
    /// [`el_wgetc`] reports end of file the moment the macro queue runs dry,
    /// which is what gives the walk a deterministic bottom.
    fn el() -> EditLine {
        let mut el = blank_editline();
        el.el_infd = -1;
        el.el_outfd = -1;
        el.el_errfd = -1;
        assert_eq!(read_init(&mut el), 0);
        el
    }

    fn bind(el: &mut EditLine, key: &str, cmd: i32) {
        let val = keymacro_map_cmd(el, cmd);
        keymacro_add(el, &w(key), &val, XK_CMD);
    }

    /// Walk the trie the way `node_trav` does but without reading input:
    /// follow the sibling chain for each character, then descend `next`.
    fn walk<'a>(el: &'a EditLine, key: &str) -> Option<&'a KeymacroNodeT> {
        let key = w(key);
        let mut node = el.el_keymacro.map.as_deref()?;
        for (i, &c) in key.iter().enumerate() {
            while node.ch != c {
                node = node.sibling.as_deref()?;
            }
            if i + 1 == key.len() {
                return Some(node);
            }
            node = node.next.as_deref()?;
        }
        None
    }

    /// The command a complete key resolves to, or `None`. A node is a binding
    /// only when it has no children: `node_trav` tests `next` before it looks
    /// at `type`, which is what hides a short key behind a longer one.
    fn bound(el: &EditLine, key: &str) -> Option<i32> {
        match walk(el, key) {
            Some(n) if n.next.is_none() => match n.val {
                KeymacroValueT::Cmd(c) => Some(i32::from(c)),
                KeymacroValueT::Str(_) => None,
            },
            _ => None,
        }
    }

    /// Deleting a key deletes every longer key that has it as a prefix, and
    /// the only thing this function ever reports is the empty-key rejection —
    /// "was anything actually deleted?" is discarded before any caller sees
    /// it, so an unbound key and a successful delete are the same 0.
    // [spec:libedit:sem:keymacro.keymacro-delete-fn/test]
    #[test]
    fn deleting_a_key_takes_every_longer_key_with_it() {
        let mut el = el();

        // Step 2: an empty trie is 0, not an error.
        assert_eq!(keymacro_delete(&mut el, &w("a")), 0);

        bind(&mut el, "abc", 11);
        bind(&mut el, "abd", 12);
        bind(&mut el, "xy", 13);
        assert_eq!(bound(&el, "abc"), Some(11));

        assert_eq!(keymacro_delete(&mut el, &w("ab")), 0);
        assert_eq!(bound(&el, "abc"), None);
        assert_eq!(bound(&el, "abd"), None);
        assert!(walk(&el, "a").is_none(), "the prefix node is pruned too");
        assert_eq!(bound(&el, "xy"), Some(13), "an unrelated key is untouched");

        // The same 0 for a key that was never bound.
        assert_eq!(keymacro_delete(&mut el, &w("zz")), 0);
        assert_eq!(bound(&el, "xy"), Some(13));

        // Step 1, the function's only -1, and it changes nothing.
        assert_eq!(keymacro_delete(&mut el, &[]), -1);
        assert_eq!(keymacro_delete(&mut el, &[0]), -1, "a bare terminator too");
        assert_eq!(bound(&el, "xy"), Some(13));
    }

    /// Unlinking before freeing is load-bearing and silent when it is wrong:
    /// `node_put` follows sibling links, so a victim released while still
    /// pointing at the rest of its level takes that level with it. Deleting
    /// the middle of a three-way chain is the shape that catches it — a
    /// differential would need three keys sharing a prefix and a lookup of
    /// each afterwards to notice.
    // [spec:libedit:sem:keymacro.node-delete-fn/test]
    #[test]
    fn deleting_one_sibling_leaves_the_rest_of_the_chain() {
        let mut el = el();
        bind(&mut el, "ab", 1);
        bind(&mut el, "ac", 2);
        bind(&mut el, "ad", 3);

        assert_eq!(keymacro_delete(&mut el, &w("ac")), 0);
        assert_eq!(bound(&el, "ab"), Some(1));
        assert_eq!(bound(&el, "ad"), Some(3));
        assert_eq!(bound(&el, "ac"), None);

        // The head of the chain, where the link slot being rewritten is the
        // parent's `next` rather than a sibling field.
        assert_eq!(keymacro_delete(&mut el, &w("ab")), 0);
        assert_eq!(bound(&el, "ad"), Some(3));
        assert!(walk(&el, "a").is_some(), "the level still has a member");

        // The last one takes the parent with it, and the prune runs to the
        // root: a node kept alive only by children it no longer has is freed.
        assert_eq!(keymacro_delete(&mut el, &w("ad")), 0);
        assert!(el.el_keymacro.map.is_none());
    }

    /// ERR-input-32, the destructive half of the shadowing rule. Binding
    /// `"ab"` and then `"abc"` leaves the shorter binding allocated but
    /// unreachable, because `node_try` step 3 does not touch `type` — and
    /// deleting the longer key then prunes the node that carried the shorter
    /// one rather than restoring it. Both are gone, and nothing says so.
    #[test]
    fn a_shadowed_binding_is_pruned_rather_than_restored() {
        let mut el = el();
        bind(&mut el, "ab", 7);
        assert_eq!(bound(&el, "ab"), Some(7));

        bind(&mut el, "abc", 8);
        assert_eq!(bound(&el, "abc"), Some(8));
        assert_eq!(bound(&el, "ab"), None, "the shorter key is shadowed");
        // Still there, and still holding its command — just unreachable.
        let ab = walk(&el, "ab").expect("the node survives");
        assert!(matches!(ab.val, KeymacroValueT::Cmd(7)));

        assert_eq!(keymacro_delete(&mut el, &w("abc")), 0);
        assert_eq!(bound(&el, "ab"), None, "the prune took the shadowed one");
        assert!(el.el_keymacro.map.is_none());
    }

    /// The other direction of the shadowing rule: binding a key that is a
    /// proper prefix of existing longer keys frees the whole child level and
    /// every payload under it (`node_try` step 2a). The file header warns
    /// about it; nothing at run time does.
    #[test]
    fn binding_a_prefix_destroys_every_longer_key_under_it() {
        let mut el = el();
        bind(&mut el, "abcd", 1);
        bind(&mut el, "abce", 2);
        bind(&mut el, "abz", 3);

        bind(&mut el, "abc", 9);
        assert_eq!(bound(&el, "abc"), Some(9));
        assert_eq!(bound(&el, "abcd"), None);
        assert_eq!(bound(&el, "abce"), None);
        assert_eq!(bound(&el, "abz"), Some(3), "a sibling level is untouched");
    }

    /// A prefix of a bound sequence is not the binding: the walk blocks for
    /// another character instead, and there is no "partial match" return code
    /// to say so. When the next character does not continue the sequence the
    /// answer is `XK_STR` with an *empty* payload — the same code a real
    /// macro binding uses (ERR-input-33), so a caller that does not test the
    /// payload reads a dead end as a macro — and every character consumed on
    /// the way in is dropped with no pushback.
    ///
    /// The characters come from the macro queue, which `el_wgetc` drains
    /// before it touches the tty; with `el_infd` closed off the queue running
    /// dry is end of file, which is the `XK_NOD` below.
    // [spec:libedit:sem:keymacro.node-trav-fn/test]
    #[test]
    fn a_prefix_of_a_bound_sequence_is_not_the_binding() {
        let mut el = el();
        bind(&mut el, "abc", 42);

        // The whole sequence: the leaf answers, and a command binding keeps
        // the character that completed it rather than clearing `ch`.
        el_wpush(&mut el, Some(&w("bc")));
        let mut ch = u32::from(b'a');
        let mut val = KeymacroValueT::Str(Vec::new());
        assert_eq!(keymacro_get(&mut el, &mut ch, &mut val), XK_CMD);
        assert!(matches!(val, KeymacroValueT::Cmd(42)));
        assert_eq!(ch, u32::from(b'c'));

        // A dead end one character in, reported as an empty macro.
        el_wpush(&mut el, Some(&w("bx")));
        let mut ch = u32::from(b'a');
        assert_eq!(keymacro_get(&mut el, &mut ch, &mut val), XK_STR);
        assert!(
            matches!(&val, KeymacroValueT::Str(s) if s.is_empty()),
            "no match and a real empty macro are the same answer"
        );

        // The proper prefix itself: no more input, so the walk reports end of
        // file rather than the interior node it is standing on.
        el_wpush(&mut el, Some(&w("b")));
        let mut ch = u32::from(b'a');
        assert_eq!(keymacro_get(&mut el, &mut ch, &mut val), XK_NOD);
    }
}
