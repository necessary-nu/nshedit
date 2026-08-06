//! Ported from `src/map.c`; rules live in `docs/spec/port/src/map.md`.

use std::borrow::Cow;
use std::io::Write;
use std::mem::ManuallyDrop;
use std::os::fd::FromRawFd;

use crate::el::{EL_BUFSIZ, EditLine, ElActionT};
// The generated command table. A glob import because the three keymap tables
// below name 86 of the 96 commands one by one; `crate::fcns` is generated
// output and publishes nothing else.
use crate::fcns::*;
use crate::keymacro::{
    keymacro__decode_str, keymacro_add, keymacro_clear, keymacro_delete, keymacro_map_cmd,
    keymacro_map_str, keymacro_print, keymacro_reset,
};
use crate::locale;
use crate::parse::{parse__string, parse_cmd};
use crate::terminal::{
    terminal_bind_arrow, terminal_clear_arrow, terminal_print_arrow, terminal_set_arrow,
};
use crate::tty::tty_bind_char;

/// C: `#define MAP_EMACS 0` — the value of `el_map.type`, an independent
/// mode tag. Note this is *not* how "are we in vi command mode" is tested;
/// that is `el_map.current == el_map.alt`, per `sem:map.map-init-fn`.
pub(crate) const MAP_EMACS: i32 = 0;
/// C: `#define MAP_VI 1`.
pub(crate) const MAP_VI: i32 = 1;

/// C: `#define N_KEYS 256` — the size of every key map.
pub const N_KEYS: usize = 256;

// Constants the C reaches through headers that have no Rust home yet, as
// `hist.rs` puts it: private here, and idiomatization should fold each into
// the module that ends up owning its header.

/// C: `#define XK_CMD 0` (`keymacro.h`) — the value argument is a command
/// number. `crate::keymacro` models the *value* as an enum but still takes the
/// tag as an `i32`, so the tag is still needed at every call.
const XK_CMD: i32 = 0;
/// C: `#define XK_STR 1` (`keymacro.h`) — the value argument is a macro string.
const XK_STR: i32 = 1;

/// C: `#define STRQQ "\"\""` (`chared.h`) — the separator that makes
/// `keymacro__decode_str` wrap its rendering in double quotes.
const STRQQ: &[u8] = b"\"\"";

/// C: `CONTROL('X')` (`tty.h`: `(A) & 037`) — the lead-in of emacs mode's one
/// compiled-in keymacro.
const CONTROL_X: u32 = 0x18;

// [spec:libedit:def:map.el-func-t-edit-line-wint-t]
/// C: `typedef el_action_t (*el_func_t)(EditLine *, wint_t);`
///
/// An editor command, as stored in [`ElMapT::func`] and dispatched by
/// `el_wgets`. It is the C ABI's shape because `el_set(EL_ADDFN, name, help,
/// func)` lets an application add one: what arrives there is an `extern "C"`
/// function pointer, `EditLine *` is `*mut EditLine` rather than
/// `&mut EditLine`, and the call is `unsafe` because the command may be the
/// caller's code. The `wint_t` is the character that invoked it and stays
/// `u32` — the crate's uniform carrier for `wchar_t`/`wint_t` — and
/// `(wint_t)-1` does reach these functions.
///
/// The port's own 96 commands are ordinary Rust functions taking
/// `&mut EditLine` (`plan/decisions/idiomatic-core.md`), so they are not
/// values of this type. [`el_func!`] is the one-line trampoline that makes
/// each one into a value of it, and `crate::fcns::EL_FUNC` is the table it
/// builds. Nothing observable depends on the identity of a stored pointer —
/// no rule compares `el_map.func[n]` against anything — so the indirection is
/// invisible across the ABI.
pub type ElFuncT = unsafe extern "C" fn(*mut EditLine, u32) -> ElActionT;

/// Wraps one of the port's own editor commands as an [`ElFuncT`].
///
/// In C there is one ABI, so `el_func[]` holds the command functions
/// themselves. Here the table has to hold C-callable pointers while the
/// commands stay idiomatic, and this is the join: one `extern "C"` shim per
/// table row, generated at the use site so each row still names its command
/// exactly once.
macro_rules! el_func {
    ($cmd:path) => {{
        /// # Safety
        ///
        /// `el` must be the live `EditLine` libedit is dispatching for, which
        /// is what `sem:map.map-init-fn` installs this table against and what
        /// `sem:read.el-wgets-fn` passes at every dispatch.
        unsafe extern "C" fn shim(el: *mut $crate::el::EditLine, c: u32) -> $crate::el::ElActionT {
            // SAFETY: the caller's obligation above; the command borrows the
            // handle only for the duration of this call.
            $cmd(unsafe { &mut *el }, c)
        }
        shim as $crate::map::ElFuncT
    }};
}
pub(crate) use el_func;

// [spec:libedit:def:map.el-bindings-t]
/// One row of the help table, for the `bind` shell command.
///
/// `name` and `description` are `Cow` because the C's are: `map_init`
/// `memcpy`s the generated static `el_func_help[]`, whose strings are wide
/// literals, and `map_addfunc` appends rows whose strings are `wcsdup`ed
/// from the caller. `map_end` frees only the appended ones — the borrowed
/// versus owned distinction the C makes by index, made structural.
pub struct ElBindingsT {
    /// Function numeric value.
    pub func: i32,
    /// C: `const wchar_t *name` — function name for the bind command.
    pub name: Cow<'static, [u32]>,
    /// C: `const wchar_t *description` — description of the function.
    pub description: Cow<'static, [u32]>,
}

/// Which of `el_map_t`'s two live maps `current` designates.
///
/// C: `el_action_t *current` aliases either `key` or `alt`. Rust cannot hold
/// a second mutable alias, so the alias becomes a selector. Note that
/// `chared.c` twice tests `el->el_map.current != el->el_map.emacs`, which is
/// unconditionally true in the C because `current` is only ever `key` or
/// `alt`; leaving `Emacs` out of this enum preserves that outcome rather
/// than inviting a translation that could make the test meaningful.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ElMapCurrent {
    /// `current == el_map.key`.
    Key,
    /// `current == el_map.alt`.
    Alt,
}

// [spec:libedit:def:map.el-map-t]
/// The key maps and the editor function tables.
pub struct ElMapT {
    /// C: `el_action_t *alt` — the current alternate key map, owned,
    /// `N_KEYS` entries.
    pub alt: Vec<ElActionT>,
    /// C: `el_action_t *key` — the current normal key map, owned, `N_KEYS`
    /// entries.
    pub key: Vec<ElActionT>,
    /// The keymap we are using — an alias of `key` or `alt`, so a selector.
    pub current: ElMapCurrent,
    /// C: `const el_action_t *emacs` — the default emacs key map, a
    /// compiled-in table. `map_end` sets it to NULL, hence the `Option`.
    pub emacs: Option<&'static [ElActionT; N_KEYS]>,
    /// The vi command mode key map, likewise compiled in.
    pub vic: Option<&'static [ElActionT; N_KEYS]>,
    /// The vi insert mode key map, likewise compiled in.
    pub vii: Option<&'static [ElActionT; N_KEYS]>,
    /// C: `int type` — `MAP_EMACS` (0) or `MAP_VI` (1). Left an integer;
    /// the C treats it as one.
    pub r#type: i32,
    /// The help for the editor functions, owned, `nfunc` entries.
    pub help: Vec<ElBindingsT>,
    /// List of available functions, owned, `nfunc` entries.
    pub func: Vec<ElFuncT>,
    /// The number of functions/help items. Retained even though `help` and
    /// `func` know their own lengths, because the `sem` rules index by it
    /// and because the C lets the two arrays and this counter disagree on
    /// `map_addfunc`'s failure paths.
    pub nfunc: usize,
    /// C: `wchar_t *wordchars` — the word character separators, owned.
    /// NULL until `map_init` runs and after `map_end`;
    /// `sem:map.el-get-fn` notes that
    /// `el_get(EL_WORDCHARS, &p)` hands out a pointer into this, so the
    /// port must copy rather than alias.
    pub wordchars: Option<Vec<u32>>,
}

// ---------------------------------------------------------------------------
// The three compiled-in keymaps, transcribed verbatim from `src/map.c`. They
// are `static` and never written through: `el_map.emacs`, `el_map.vic` and
// `el_map.vii` borrow them for the life of the process, and only the two heap
// maps (`key`, `alt`) are ever mutated. That is the invariant
// `sem:map.map-init-fn` spells out and that `chared`'s
// `current != emacs` tautology rests on.
// ---------------------------------------------------------------------------

/// C: `static const el_action_t el_map_emacs[]` — the default emacs key
/// map, `el_map.emacs`.
///
/// `ERR-modes-70` — the C's numeric index comments are wrong from `M-_`
/// onward: two entries are both labelled 223 and the labels lag by one
/// thereafter, ending with `254` on the actual index 255. The indices here
/// are the real ones; the character comments are the C's, which are correct.
static EL_MAP_EMACS: [ElActionT; N_KEYS] = [
    EM_SET_MARK,            // 0 ^@
    ED_MOVE_TO_BEG,         // 1 ^A
    ED_PREV_CHAR,           // 2 ^B
    ED_IGNORE,              // 3 ^C
    EM_DELETE_OR_LIST,      // 4 ^D
    ED_MOVE_TO_END,         // 5 ^E
    ED_NEXT_CHAR,           // 6 ^F
    ED_UNASSIGNED,          // 7 ^G
    EM_DELETE_PREV_CHAR,    // 8 ^H
    ED_UNASSIGNED,          // 9 ^I
    ED_NEWLINE,             // 10 ^J
    ED_KILL_LINE,           // 11 ^K
    ED_CLEAR_SCREEN,        // 12 ^L
    ED_NEWLINE,             // 13 ^M
    ED_NEXT_HISTORY,        // 14 ^N
    ED_IGNORE,              // 15 ^O
    ED_PREV_HISTORY,        // 16 ^P
    ED_IGNORE,              // 17 ^Q
    EM_INC_SEARCH_PREV,     // 18 ^R
    ED_IGNORE,              // 19 ^S
    ED_TRANSPOSE_CHARS,     // 20 ^T
    EM_KILL_LINE,           // 21 ^U
    ED_QUOTED_INSERT,       // 22 ^V
    ED_DELETE_PREV_WORD,    // 23 ^W
    ED_SEQUENCE_LEAD_IN,    // 24 ^X
    EM_YANK,                // 25 ^Y
    ED_IGNORE,              // 26 ^Z
    EM_META_NEXT,           // 27 ^[
    ED_IGNORE,              // 28 ^\
    ED_IGNORE,              // 29 ^]
    ED_UNASSIGNED,          // 30 ^^
    ED_UNASSIGNED,          // 31 ^_
    ED_INSERT,              // 32 SPACE
    ED_INSERT,              // 33 !
    ED_INSERT,              // 34 "
    ED_INSERT,              // 35 #
    ED_INSERT,              // 36 $
    ED_INSERT,              // 37 %
    ED_INSERT,              // 38 &
    ED_INSERT,              // 39 '
    ED_INSERT,              // 40 (
    ED_INSERT,              // 41 )
    ED_INSERT,              // 42 *
    ED_INSERT,              // 43 +
    ED_INSERT,              // 44 ,
    ED_INSERT,              // 45 -
    ED_INSERT,              // 46 .
    ED_INSERT,              // 47 /
    ED_DIGIT,               // 48 0
    ED_DIGIT,               // 49 1
    ED_DIGIT,               // 50 2
    ED_DIGIT,               // 51 3
    ED_DIGIT,               // 52 4
    ED_DIGIT,               // 53 5
    ED_DIGIT,               // 54 6
    ED_DIGIT,               // 55 7
    ED_DIGIT,               // 56 8
    ED_DIGIT,               // 57 9
    ED_INSERT,              // 58 :
    ED_INSERT,              // 59 ;
    ED_INSERT,              // 60 <
    ED_INSERT,              // 61 =
    ED_INSERT,              // 62 >
    ED_INSERT,              // 63 ?
    ED_INSERT,              // 64 @
    ED_INSERT,              // 65 A
    ED_INSERT,              // 66 B
    ED_INSERT,              // 67 C
    ED_INSERT,              // 68 D
    ED_INSERT,              // 69 E
    ED_INSERT,              // 70 F
    ED_INSERT,              // 71 G
    ED_INSERT,              // 72 H
    ED_INSERT,              // 73 I
    ED_INSERT,              // 74 J
    ED_INSERT,              // 75 K
    ED_INSERT,              // 76 L
    ED_INSERT,              // 77 M
    ED_INSERT,              // 78 N
    ED_INSERT,              // 79 O
    ED_INSERT,              // 80 P
    ED_INSERT,              // 81 Q
    ED_INSERT,              // 82 R
    ED_INSERT,              // 83 S
    ED_INSERT,              // 84 T
    ED_INSERT,              // 85 U
    ED_INSERT,              // 86 V
    ED_INSERT,              // 87 W
    ED_INSERT,              // 88 X
    ED_INSERT,              // 89 Y
    ED_INSERT,              // 90 Z
    ED_INSERT,              // 91 [
    ED_INSERT,              // 92 \
    ED_INSERT,              // 93 ]
    ED_INSERT,              // 94 ^
    ED_INSERT,              // 95 _
    ED_INSERT,              // 96 `
    ED_INSERT,              // 97 a
    ED_INSERT,              // 98 b
    ED_INSERT,              // 99 c
    ED_INSERT,              // 100 d
    ED_INSERT,              // 101 e
    ED_INSERT,              // 102 f
    ED_INSERT,              // 103 g
    ED_INSERT,              // 104 h
    ED_INSERT,              // 105 i
    ED_INSERT,              // 106 j
    ED_INSERT,              // 107 k
    ED_INSERT,              // 108 l
    ED_INSERT,              // 109 m
    ED_INSERT,              // 110 n
    ED_INSERT,              // 111 o
    ED_INSERT,              // 112 p
    ED_INSERT,              // 113 q
    ED_INSERT,              // 114 r
    ED_INSERT,              // 115 s
    ED_INSERT,              // 116 t
    ED_INSERT,              // 117 u
    ED_INSERT,              // 118 v
    ED_INSERT,              // 119 w
    ED_INSERT,              // 120 x
    ED_INSERT,              // 121 y
    ED_INSERT,              // 122 z
    ED_INSERT,              // 123 {
    ED_INSERT,              // 124 |
    ED_INSERT,              // 125 }
    ED_INSERT,              // 126 ~
    EM_DELETE_PREV_CHAR,    // 127 ^?
    ED_UNASSIGNED,          // 128 M-^@
    ED_UNASSIGNED,          // 129 M-^A
    ED_UNASSIGNED,          // 130 M-^B
    ED_UNASSIGNED,          // 131 M-^C
    ED_UNASSIGNED,          // 132 M-^D
    ED_UNASSIGNED,          // 133 M-^E
    ED_UNASSIGNED,          // 134 M-^F
    ED_UNASSIGNED,          // 135 M-^G
    ED_DELETE_PREV_WORD,    // 136 M-^H
    ED_UNASSIGNED,          // 137 M-^I
    ED_UNASSIGNED,          // 138 M-^J
    ED_UNASSIGNED,          // 139 M-^K
    ED_CLEAR_SCREEN,        // 140 M-^L
    ED_UNASSIGNED,          // 141 M-^M
    ED_UNASSIGNED,          // 142 M-^N
    ED_UNASSIGNED,          // 143 M-^O
    ED_UNASSIGNED,          // 144 M-^P
    ED_UNASSIGNED,          // 145 M-^Q
    ED_UNASSIGNED,          // 146 M-^R
    ED_UNASSIGNED,          // 147 M-^S
    ED_UNASSIGNED,          // 148 M-^T
    ED_UNASSIGNED,          // 149 M-^U
    ED_UNASSIGNED,          // 150 M-^V
    ED_UNASSIGNED,          // 151 M-^W
    ED_UNASSIGNED,          // 152 M-^X
    ED_UNASSIGNED,          // 153 M-^Y
    ED_UNASSIGNED,          // 154 M-^Z
    ED_UNASSIGNED,          // 155 M-^[
    ED_UNASSIGNED,          // 156 M-^\
    ED_UNASSIGNED,          // 157 M-^]
    ED_UNASSIGNED,          // 158 M-^^
    EM_COPY_PREV_WORD,      // 159 M-^_
    ED_UNASSIGNED,          // 160 M-SPACE
    ED_UNASSIGNED,          // 161 M-!
    ED_UNASSIGNED,          // 162 M-"
    ED_UNASSIGNED,          // 163 M-#
    ED_UNASSIGNED,          // 164 M-$
    ED_UNASSIGNED,          // 165 M-%
    ED_UNASSIGNED,          // 166 M-&
    ED_UNASSIGNED,          // 167 M-'
    ED_UNASSIGNED,          // 168 M-(
    ED_UNASSIGNED,          // 169 M-)
    ED_UNASSIGNED,          // 170 M-*
    ED_UNASSIGNED,          // 171 M-+
    ED_UNASSIGNED,          // 172 M-,
    ED_UNASSIGNED,          // 173 M--
    ED_UNASSIGNED,          // 174 M-.
    ED_UNASSIGNED,          // 175 M-/
    ED_ARGUMENT_DIGIT,      // 176 M-0
    ED_ARGUMENT_DIGIT,      // 177 M-1
    ED_ARGUMENT_DIGIT,      // 178 M-2
    ED_ARGUMENT_DIGIT,      // 179 M-3
    ED_ARGUMENT_DIGIT,      // 180 M-4
    ED_ARGUMENT_DIGIT,      // 181 M-5
    ED_ARGUMENT_DIGIT,      // 182 M-6
    ED_ARGUMENT_DIGIT,      // 183 M-7
    ED_ARGUMENT_DIGIT,      // 184 M-8
    ED_ARGUMENT_DIGIT,      // 185 M-9
    ED_UNASSIGNED,          // 186 M-:
    ED_UNASSIGNED,          // 187 M-;
    ED_UNASSIGNED,          // 188 M-<
    ED_UNASSIGNED,          // 189 M-=
    ED_UNASSIGNED,          // 190 M->
    ED_UNASSIGNED,          // 191 M-?
    ED_UNASSIGNED,          // 192 M-@
    ED_UNASSIGNED,          // 193 M-A
    ED_PREV_WORD,           // 194 M-B
    EM_CAPITOL_CASE,        // 195 M-C
    EM_DELETE_NEXT_WORD,    // 196 M-D
    ED_UNASSIGNED,          // 197 M-E
    EM_NEXT_WORD,           // 198 M-F
    ED_UNASSIGNED,          // 199 M-G
    ED_UNASSIGNED,          // 200 M-H
    ED_UNASSIGNED,          // 201 M-I
    ED_UNASSIGNED,          // 202 M-J
    ED_UNASSIGNED,          // 203 M-K
    EM_LOWER_CASE,          // 204 M-L
    ED_UNASSIGNED,          // 205 M-M
    ED_SEARCH_NEXT_HISTORY, // 206 M-N
    ED_SEQUENCE_LEAD_IN,    // 207 M-O
    ED_SEARCH_PREV_HISTORY, // 208 M-P
    ED_UNASSIGNED,          // 209 M-Q
    ED_UNASSIGNED,          // 210 M-R
    ED_UNASSIGNED,          // 211 M-S
    ED_UNASSIGNED,          // 212 M-T
    EM_UPPER_CASE,          // 213 M-U
    ED_UNASSIGNED,          // 214 M-V
    EM_COPY_REGION,         // 215 M-W
    ED_COMMAND,             // 216 M-X
    ED_UNASSIGNED,          // 217 M-Y
    ED_UNASSIGNED,          // 218 M-Z
    ED_SEQUENCE_LEAD_IN,    // 219 M-[
    ED_UNASSIGNED,          // 220 M-\
    ED_UNASSIGNED,          // 221 M-]
    ED_UNASSIGNED,          // 222 M-^
    ED_UNASSIGNED,          // 223 M-_
    ED_UNASSIGNED,          // 224 M-`
    ED_UNASSIGNED,          // 225 M-a
    ED_PREV_WORD,           // 226 M-b
    EM_CAPITOL_CASE,        // 227 M-c
    EM_DELETE_NEXT_WORD,    // 228 M-d
    ED_UNASSIGNED,          // 229 M-e
    EM_NEXT_WORD,           // 230 M-f
    ED_UNASSIGNED,          // 231 M-g
    ED_UNASSIGNED,          // 232 M-h
    ED_UNASSIGNED,          // 233 M-i
    ED_UNASSIGNED,          // 234 M-j
    ED_UNASSIGNED,          // 235 M-k
    EM_LOWER_CASE,          // 236 M-l
    ED_UNASSIGNED,          // 237 M-m
    ED_SEARCH_NEXT_HISTORY, // 238 M-n
    ED_UNASSIGNED,          // 239 M-o
    ED_SEARCH_PREV_HISTORY, // 240 M-p
    ED_UNASSIGNED,          // 241 M-q
    ED_UNASSIGNED,          // 242 M-r
    ED_UNASSIGNED,          // 243 M-s
    ED_UNASSIGNED,          // 244 M-t
    EM_UPPER_CASE,          // 245 M-u
    ED_UNASSIGNED,          // 246 M-v
    EM_COPY_REGION,         // 247 M-w
    ED_COMMAND,             // 248 M-x
    ED_UNASSIGNED,          // 249 M-y
    ED_UNASSIGNED,          // 250 M-z
    ED_UNASSIGNED,          // 251 M-{
    ED_UNASSIGNED,          // 252 M-|
    ED_UNASSIGNED,          // 253 M-}
    ED_UNASSIGNED,          // 254 M-~
    ED_DELETE_PREV_WORD,    // 255 M-^?
];

/// C: `static const el_action_t el_map_vi_insert[]` — the default vi
/// *insert* map, `el_map.vii`.
///
/// `KSHVI` is defined unconditionally in `el.h`, so indices 0..31 come from
/// the `#ifdef KSHVI` branch; the `#else` block is dead code and is not
/// ported (`ERR-modes-71`).
static EL_MAP_VI_INSERT: [ElActionT; N_KEYS] = [
    ED_UNASSIGNED,       // 0 ^@
    ED_INSERT,           // 1 ^A
    ED_INSERT,           // 2 ^B
    ED_INSERT,           // 3 ^C
    VI_LIST_OR_EOF,      // 4 ^D
    ED_INSERT,           // 5 ^E
    ED_INSERT,           // 6 ^F
    ED_INSERT,           // 7 ^G
    VI_DELETE_PREV_CHAR, // 8 ^H  BackSpace key
    ED_INSERT,           // 9 ^I  Tab Key
    ED_NEWLINE,          // 10 ^J
    ED_INSERT,           // 11 ^K
    ED_INSERT,           // 12 ^L
    ED_NEWLINE,          // 13 ^M
    ED_INSERT,           // 14 ^N
    ED_INSERT,           // 15 ^O
    ED_INSERT,           // 16 ^P
    ED_IGNORE,           // 17 ^Q
    ED_INSERT,           // 18 ^R
    ED_IGNORE,           // 19 ^S
    ED_INSERT,           // 20 ^T
    VI_KILL_LINE_PREV,   // 21 ^U
    ED_QUOTED_INSERT,    // 22 ^V
    ED_DELETE_PREV_WORD, // 23 ^W
    ED_INSERT,           // 24 ^X
    ED_INSERT,           // 25 ^Y
    ED_INSERT,           // 26 ^Z
    VI_COMMAND_MODE,     // 27 ^[  [ Esc ] key
    ED_IGNORE,           // 28 ^\
    ED_INSERT,           // 29 ^]
    ED_INSERT,           // 30 ^^
    ED_INSERT,           // 31 ^_
    ED_INSERT,           // 32 SPACE
    ED_INSERT,           // 33 !
    ED_INSERT,           // 34 "
    ED_INSERT,           // 35 #
    ED_INSERT,           // 36 $
    ED_INSERT,           // 37 %
    ED_INSERT,           // 38 &
    ED_INSERT,           // 39 '
    ED_INSERT,           // 40 (
    ED_INSERT,           // 41 )
    ED_INSERT,           // 42 *
    ED_INSERT,           // 43 +
    ED_INSERT,           // 44 ,
    ED_INSERT,           // 45 -
    ED_INSERT,           // 46 .
    ED_INSERT,           // 47 /
    ED_INSERT,           // 48 0
    ED_INSERT,           // 49 1
    ED_INSERT,           // 50 2
    ED_INSERT,           // 51 3
    ED_INSERT,           // 52 4
    ED_INSERT,           // 53 5
    ED_INSERT,           // 54 6
    ED_INSERT,           // 55 7
    ED_INSERT,           // 56 8
    ED_INSERT,           // 57 9
    ED_INSERT,           // 58 :
    ED_INSERT,           // 59 ;
    ED_INSERT,           // 60 <
    ED_INSERT,           // 61 =
    ED_INSERT,           // 62 >
    ED_INSERT,           // 63 ?
    ED_INSERT,           // 64 @
    ED_INSERT,           // 65 A
    ED_INSERT,           // 66 B
    ED_INSERT,           // 67 C
    ED_INSERT,           // 68 D
    ED_INSERT,           // 69 E
    ED_INSERT,           // 70 F
    ED_INSERT,           // 71 G
    ED_INSERT,           // 72 H
    ED_INSERT,           // 73 I
    ED_INSERT,           // 74 J
    ED_INSERT,           // 75 K
    ED_INSERT,           // 76 L
    ED_INSERT,           // 77 M
    ED_INSERT,           // 78 N
    ED_INSERT,           // 79 O
    ED_INSERT,           // 80 P
    ED_INSERT,           // 81 Q
    ED_INSERT,           // 82 R
    ED_INSERT,           // 83 S
    ED_INSERT,           // 84 T
    ED_INSERT,           // 85 U
    ED_INSERT,           // 86 V
    ED_INSERT,           // 87 W
    ED_INSERT,           // 88 X
    ED_INSERT,           // 89 Y
    ED_INSERT,           // 90 Z
    ED_INSERT,           // 91 [
    ED_INSERT,           // 92 \
    ED_INSERT,           // 93 ]
    ED_INSERT,           // 94 ^
    ED_INSERT,           // 95 _
    ED_INSERT,           // 96 `
    ED_INSERT,           // 97 a
    ED_INSERT,           // 98 b
    ED_INSERT,           // 99 c
    ED_INSERT,           // 100 d
    ED_INSERT,           // 101 e
    ED_INSERT,           // 102 f
    ED_INSERT,           // 103 g
    ED_INSERT,           // 104 h
    ED_INSERT,           // 105 i
    ED_INSERT,           // 106 j
    ED_INSERT,           // 107 k
    ED_INSERT,           // 108 l
    ED_INSERT,           // 109 m
    ED_INSERT,           // 110 n
    ED_INSERT,           // 111 o
    ED_INSERT,           // 112 p
    ED_INSERT,           // 113 q
    ED_INSERT,           // 114 r
    ED_INSERT,           // 115 s
    ED_INSERT,           // 116 t
    ED_INSERT,           // 117 u
    ED_INSERT,           // 118 v
    ED_INSERT,           // 119 w
    ED_INSERT,           // 120 x
    ED_INSERT,           // 121 y
    ED_INSERT,           // 122 z
    ED_INSERT,           // 123 {
    ED_INSERT,           // 124 |
    ED_INSERT,           // 125 }
    ED_INSERT,           // 126 ~
    VI_DELETE_PREV_CHAR, // 127 ^?
    ED_INSERT,           // 128 M-^@
    ED_INSERT,           // 129 M-^A
    ED_INSERT,           // 130 M-^B
    ED_INSERT,           // 131 M-^C
    ED_INSERT,           // 132 M-^D
    ED_INSERT,           // 133 M-^E
    ED_INSERT,           // 134 M-^F
    ED_INSERT,           // 135 M-^G
    ED_INSERT,           // 136 M-^H
    ED_INSERT,           // 137 M-^I
    ED_INSERT,           // 138 M-^J
    ED_INSERT,           // 139 M-^K
    ED_INSERT,           // 140 M-^L
    ED_INSERT,           // 141 M-^M
    ED_INSERT,           // 142 M-^N
    ED_INSERT,           // 143 M-^O
    ED_INSERT,           // 144 M-^P
    ED_INSERT,           // 145 M-^Q
    ED_INSERT,           // 146 M-^R
    ED_INSERT,           // 147 M-^S
    ED_INSERT,           // 148 M-^T
    ED_INSERT,           // 149 M-^U
    ED_INSERT,           // 150 M-^V
    ED_INSERT,           // 151 M-^W
    ED_INSERT,           // 152 M-^X
    ED_INSERT,           // 153 M-^Y
    ED_INSERT,           // 154 M-^Z
    ED_INSERT,           // 155 M-^[
    ED_INSERT,           // 156 M-^\
    ED_INSERT,           // 157 M-^]
    ED_INSERT,           // 158 M-^^
    ED_INSERT,           // 159 M-^_
    ED_INSERT,           // 160 M-SPACE
    ED_INSERT,           // 161 M-!
    ED_INSERT,           // 162 M-"
    ED_INSERT,           // 163 M-#
    ED_INSERT,           // 164 M-$
    ED_INSERT,           // 165 M-%
    ED_INSERT,           // 166 M-&
    ED_INSERT,           // 167 M-'
    ED_INSERT,           // 168 M-(
    ED_INSERT,           // 169 M-)
    ED_INSERT,           // 170 M-*
    ED_INSERT,           // 171 M-+
    ED_INSERT,           // 172 M-,
    ED_INSERT,           // 173 M--
    ED_INSERT,           // 174 M-.
    ED_INSERT,           // 175 M-/
    ED_INSERT,           // 176 M-0
    ED_INSERT,           // 177 M-1
    ED_INSERT,           // 178 M-2
    ED_INSERT,           // 179 M-3
    ED_INSERT,           // 180 M-4
    ED_INSERT,           // 181 M-5
    ED_INSERT,           // 182 M-6
    ED_INSERT,           // 183 M-7
    ED_INSERT,           // 184 M-8
    ED_INSERT,           // 185 M-9
    ED_INSERT,           // 186 M-:
    ED_INSERT,           // 187 M-;
    ED_INSERT,           // 188 M-<
    ED_INSERT,           // 189 M-=
    ED_INSERT,           // 190 M->
    ED_INSERT,           // 191 M-?
    ED_INSERT,           // 192 M-@
    ED_INSERT,           // 193 M-A
    ED_INSERT,           // 194 M-B
    ED_INSERT,           // 195 M-C
    ED_INSERT,           // 196 M-D
    ED_INSERT,           // 197 M-E
    ED_INSERT,           // 198 M-F
    ED_INSERT,           // 199 M-G
    ED_INSERT,           // 200 M-H
    ED_INSERT,           // 201 M-I
    ED_INSERT,           // 202 M-J
    ED_INSERT,           // 203 M-K
    ED_INSERT,           // 204 M-L
    ED_INSERT,           // 205 M-M
    ED_INSERT,           // 206 M-N
    ED_INSERT,           // 207 M-O
    ED_INSERT,           // 208 M-P
    ED_INSERT,           // 209 M-Q
    ED_INSERT,           // 210 M-R
    ED_INSERT,           // 211 M-S
    ED_INSERT,           // 212 M-T
    ED_INSERT,           // 213 M-U
    ED_INSERT,           // 214 M-V
    ED_INSERT,           // 215 M-W
    ED_INSERT,           // 216 M-X
    ED_INSERT,           // 217 M-Y
    ED_INSERT,           // 218 M-Z
    ED_INSERT,           // 219 M-[
    ED_INSERT,           // 220 M-\
    ED_INSERT,           // 221 M-]
    ED_INSERT,           // 222 M-^
    ED_INSERT,           // 223 M-_
    ED_INSERT,           // 224 M-`
    ED_INSERT,           // 225 M-a
    ED_INSERT,           // 226 M-b
    ED_INSERT,           // 227 M-c
    ED_INSERT,           // 228 M-d
    ED_INSERT,           // 229 M-e
    ED_INSERT,           // 230 M-f
    ED_INSERT,           // 231 M-g
    ED_INSERT,           // 232 M-h
    ED_INSERT,           // 233 M-i
    ED_INSERT,           // 234 M-j
    ED_INSERT,           // 235 M-k
    ED_INSERT,           // 236 M-l
    ED_INSERT,           // 237 M-m
    ED_INSERT,           // 238 M-n
    ED_INSERT,           // 239 M-o
    ED_INSERT,           // 240 M-p
    ED_INSERT,           // 241 M-q
    ED_INSERT,           // 242 M-r
    ED_INSERT,           // 243 M-s
    ED_INSERT,           // 244 M-t
    ED_INSERT,           // 245 M-u
    ED_INSERT,           // 246 M-v
    ED_INSERT,           // 247 M-w
    ED_INSERT,           // 248 M-x
    ED_INSERT,           // 249 M-y
    ED_INSERT,           // 250 M-z
    ED_INSERT,           // 251 M-{
    ED_INSERT,           // 252 M-|
    ED_INSERT,           // 253 M-}
    ED_INSERT,           // 254 M-~
    ED_INSERT,           // 255 M-^?
];

/// C: `static const el_action_t el_map_vi_command[]` — the default vi
/// *command* map, `el_map.vic`.
static EL_MAP_VI_COMMAND: [ElActionT; N_KEYS] = [
    ED_UNASSIGNED,          // 0 ^@
    ED_MOVE_TO_BEG,         // 1 ^A
    ED_UNASSIGNED,          // 2 ^B
    ED_IGNORE,              // 3 ^C
    ED_UNASSIGNED,          // 4 ^D
    ED_MOVE_TO_END,         // 5 ^E
    ED_UNASSIGNED,          // 6 ^F
    ED_UNASSIGNED,          // 7 ^G
    ED_DELETE_PREV_CHAR,    // 8 ^H
    ED_UNASSIGNED,          // 9 ^I
    ED_NEWLINE,             // 10 ^J
    ED_KILL_LINE,           // 11 ^K
    ED_CLEAR_SCREEN,        // 12 ^L
    ED_NEWLINE,             // 13 ^M
    ED_NEXT_HISTORY,        // 14 ^N
    ED_IGNORE,              // 15 ^O
    ED_PREV_HISTORY,        // 16 ^P
    ED_IGNORE,              // 17 ^Q
    ED_REDISPLAY,           // 18 ^R
    ED_IGNORE,              // 19 ^S
    ED_UNASSIGNED,          // 20 ^T
    VI_KILL_LINE_PREV,      // 21 ^U
    ED_UNASSIGNED,          // 22 ^V
    ED_DELETE_PREV_WORD,    // 23 ^W
    ED_UNASSIGNED,          // 24 ^X
    ED_UNASSIGNED,          // 25 ^Y
    ED_UNASSIGNED,          // 26 ^Z
    EM_META_NEXT,           // 27 ^[
    ED_IGNORE,              // 28 ^\
    ED_UNASSIGNED,          // 29 ^]
    ED_UNASSIGNED,          // 30 ^^
    ED_UNASSIGNED,          // 31 ^_
    ED_NEXT_CHAR,           // 32 SPACE
    ED_UNASSIGNED,          // 33 !
    ED_UNASSIGNED,          // 34 "
    VI_COMMENT_OUT,         // 35 #
    ED_MOVE_TO_END,         // 36 $
    VI_MATCH,               // 37 %
    ED_UNASSIGNED,          // 38 &
    ED_UNASSIGNED,          // 39 '
    ED_UNASSIGNED,          // 40 (
    ED_UNASSIGNED,          // 41 )
    ED_UNASSIGNED,          // 42 *
    ED_NEXT_HISTORY,        // 43 +
    VI_REPEAT_PREV_CHAR,    // 44 ,
    ED_PREV_HISTORY,        // 45 -
    VI_REDO,                // 46 .
    VI_SEARCH_PREV,         // 47 /
    VI_ZERO,                // 48 0
    ED_ARGUMENT_DIGIT,      // 49 1
    ED_ARGUMENT_DIGIT,      // 50 2
    ED_ARGUMENT_DIGIT,      // 51 3
    ED_ARGUMENT_DIGIT,      // 52 4
    ED_ARGUMENT_DIGIT,      // 53 5
    ED_ARGUMENT_DIGIT,      // 54 6
    ED_ARGUMENT_DIGIT,      // 55 7
    ED_ARGUMENT_DIGIT,      // 56 8
    ED_ARGUMENT_DIGIT,      // 57 9
    ED_COMMAND,             // 58 :
    VI_REPEAT_NEXT_CHAR,    // 59 ;
    ED_UNASSIGNED,          // 60 <
    ED_UNASSIGNED,          // 61 =
    ED_UNASSIGNED,          // 62 >
    VI_SEARCH_NEXT,         // 63 ?
    VI_ALIAS,               // 64 @
    VI_ADD_AT_EOL,          // 65 A
    VI_PREV_BIG_WORD,       // 66 B
    VI_CHANGE_TO_EOL,       // 67 C
    ED_KILL_LINE,           // 68 D
    VI_END_BIG_WORD,        // 69 E
    VI_PREV_CHAR,           // 70 F
    VI_TO_HISTORY_LINE,     // 71 G
    ED_UNASSIGNED,          // 72 H
    VI_INSERT_AT_BOL,       // 73 I
    ED_SEARCH_NEXT_HISTORY, // 74 J
    ED_SEARCH_PREV_HISTORY, // 75 K
    ED_UNASSIGNED,          // 76 L
    ED_UNASSIGNED,          // 77 M
    VI_REPEAT_SEARCH_PREV,  // 78 N
    ED_SEQUENCE_LEAD_IN,    // 79 O
    VI_PASTE_PREV,          // 80 P
    ED_UNASSIGNED,          // 81 Q
    VI_REPLACE_MODE,        // 82 R
    VI_SUBSTITUTE_LINE,     // 83 S
    VI_TO_PREV_CHAR,        // 84 T
    VI_UNDO_LINE,           // 85 U
    ED_UNASSIGNED,          // 86 V
    VI_NEXT_BIG_WORD,       // 87 W
    ED_DELETE_PREV_CHAR,    // 88 X
    VI_YANK_END,            // 89 Y
    ED_UNASSIGNED,          // 90 Z
    ED_SEQUENCE_LEAD_IN,    // 91 [
    ED_UNASSIGNED,          // 92 \
    ED_UNASSIGNED,          // 93 ]
    ED_MOVE_TO_BEG,         // 94 ^
    VI_HISTORY_WORD,        // 95 _
    ED_UNASSIGNED,          // 96 `
    VI_ADD,                 // 97 a
    VI_PREV_WORD,           // 98 b
    VI_CHANGE_META,         // 99 c
    VI_DELETE_META,         // 100 d
    VI_END_WORD,            // 101 e
    VI_NEXT_CHAR,           // 102 f
    ED_UNASSIGNED,          // 103 g
    ED_PREV_CHAR,           // 104 h
    VI_INSERT,              // 105 i
    ED_NEXT_HISTORY,        // 106 j
    ED_PREV_HISTORY,        // 107 k
    ED_NEXT_CHAR,           // 108 l
    ED_UNASSIGNED,          // 109 m
    VI_REPEAT_SEARCH_NEXT,  // 110 n
    ED_UNASSIGNED,          // 111 o
    VI_PASTE_NEXT,          // 112 p
    ED_UNASSIGNED,          // 113 q
    VI_REPLACE_CHAR,        // 114 r
    VI_SUBSTITUTE_CHAR,     // 115 s
    VI_TO_NEXT_CHAR,        // 116 t
    VI_UNDO,                // 117 u
    VI_HISTEDIT,            // 118 v
    VI_NEXT_WORD,           // 119 w
    ED_DELETE_NEXT_CHAR,    // 120 x
    VI_YANK,                // 121 y
    ED_UNASSIGNED,          // 122 z
    ED_UNASSIGNED,          // 123 {
    VI_TO_COLUMN,           // 124 |
    ED_UNASSIGNED,          // 125 }
    VI_CHANGE_CASE,         // 126 ~
    ED_DELETE_PREV_CHAR,    // 127 ^?
    ED_UNASSIGNED,          // 128 M-^@
    ED_UNASSIGNED,          // 129 M-^A
    ED_UNASSIGNED,          // 130 M-^B
    ED_UNASSIGNED,          // 131 M-^C
    ED_UNASSIGNED,          // 132 M-^D
    ED_UNASSIGNED,          // 133 M-^E
    ED_UNASSIGNED,          // 134 M-^F
    ED_UNASSIGNED,          // 135 M-^G
    ED_UNASSIGNED,          // 136 M-^H
    ED_UNASSIGNED,          // 137 M-^I
    ED_UNASSIGNED,          // 138 M-^J
    ED_UNASSIGNED,          // 139 M-^K
    ED_UNASSIGNED,          // 140 M-^L
    ED_UNASSIGNED,          // 141 M-^M
    ED_UNASSIGNED,          // 142 M-^N
    ED_UNASSIGNED,          // 143 M-^O
    ED_UNASSIGNED,          // 144 M-^P
    ED_UNASSIGNED,          // 145 M-^Q
    ED_UNASSIGNED,          // 146 M-^R
    ED_UNASSIGNED,          // 147 M-^S
    ED_UNASSIGNED,          // 148 M-^T
    ED_UNASSIGNED,          // 149 M-^U
    ED_UNASSIGNED,          // 150 M-^V
    ED_UNASSIGNED,          // 151 M-^W
    ED_UNASSIGNED,          // 152 M-^X
    ED_UNASSIGNED,          // 153 M-^Y
    ED_UNASSIGNED,          // 154 M-^Z
    ED_UNASSIGNED,          // 155 M-^[
    ED_UNASSIGNED,          // 156 M-^\
    ED_UNASSIGNED,          // 157 M-^]
    ED_UNASSIGNED,          // 158 M-^^
    ED_UNASSIGNED,          // 159 M-^_
    ED_UNASSIGNED,          // 160 M-SPACE
    ED_UNASSIGNED,          // 161 M-!
    ED_UNASSIGNED,          // 162 M-"
    ED_UNASSIGNED,          // 163 M-#
    ED_UNASSIGNED,          // 164 M-$
    ED_UNASSIGNED,          // 165 M-%
    ED_UNASSIGNED,          // 166 M-&
    ED_UNASSIGNED,          // 167 M-'
    ED_UNASSIGNED,          // 168 M-(
    ED_UNASSIGNED,          // 169 M-)
    ED_UNASSIGNED,          // 170 M-*
    ED_UNASSIGNED,          // 171 M-+
    ED_UNASSIGNED,          // 172 M-,
    ED_UNASSIGNED,          // 173 M--
    ED_UNASSIGNED,          // 174 M-.
    ED_UNASSIGNED,          // 175 M-/
    ED_UNASSIGNED,          // 176 M-0
    ED_UNASSIGNED,          // 177 M-1
    ED_UNASSIGNED,          // 178 M-2
    ED_UNASSIGNED,          // 179 M-3
    ED_UNASSIGNED,          // 180 M-4
    ED_UNASSIGNED,          // 181 M-5
    ED_UNASSIGNED,          // 182 M-6
    ED_UNASSIGNED,          // 183 M-7
    ED_UNASSIGNED,          // 184 M-8
    ED_UNASSIGNED,          // 185 M-9
    ED_UNASSIGNED,          // 186 M-:
    ED_UNASSIGNED,          // 187 M-;
    ED_UNASSIGNED,          // 188 M-<
    ED_UNASSIGNED,          // 189 M-=
    ED_UNASSIGNED,          // 190 M->
    ED_UNASSIGNED,          // 191 M-?
    ED_UNASSIGNED,          // 192 M-@
    ED_UNASSIGNED,          // 193 M-A
    ED_UNASSIGNED,          // 194 M-B
    ED_UNASSIGNED,          // 195 M-C
    ED_UNASSIGNED,          // 196 M-D
    ED_UNASSIGNED,          // 197 M-E
    ED_UNASSIGNED,          // 198 M-F
    ED_UNASSIGNED,          // 199 M-G
    ED_UNASSIGNED,          // 200 M-H
    ED_UNASSIGNED,          // 201 M-I
    ED_UNASSIGNED,          // 202 M-J
    ED_UNASSIGNED,          // 203 M-K
    ED_UNASSIGNED,          // 204 M-L
    ED_UNASSIGNED,          // 205 M-M
    ED_UNASSIGNED,          // 206 M-N
    ED_SEQUENCE_LEAD_IN,    // 207 M-O
    ED_UNASSIGNED,          // 208 M-P
    ED_UNASSIGNED,          // 209 M-Q
    ED_UNASSIGNED,          // 210 M-R
    ED_UNASSIGNED,          // 211 M-S
    ED_UNASSIGNED,          // 212 M-T
    ED_UNASSIGNED,          // 213 M-U
    ED_UNASSIGNED,          // 214 M-V
    ED_UNASSIGNED,          // 215 M-W
    ED_UNASSIGNED,          // 216 M-X
    ED_UNASSIGNED,          // 217 M-Y
    ED_UNASSIGNED,          // 218 M-Z
    ED_SEQUENCE_LEAD_IN,    // 219 M-[
    ED_UNASSIGNED,          // 220 M-\
    ED_UNASSIGNED,          // 221 M-]
    ED_UNASSIGNED,          // 222 M-^
    ED_UNASSIGNED,          // 223 M-_
    ED_UNASSIGNED,          // 224 M-`
    ED_UNASSIGNED,          // 225 M-a
    ED_UNASSIGNED,          // 226 M-b
    ED_UNASSIGNED,          // 227 M-c
    ED_UNASSIGNED,          // 228 M-d
    ED_UNASSIGNED,          // 229 M-e
    ED_UNASSIGNED,          // 230 M-f
    ED_UNASSIGNED,          // 231 M-g
    ED_UNASSIGNED,          // 232 M-h
    ED_UNASSIGNED,          // 233 M-i
    ED_UNASSIGNED,          // 234 M-j
    ED_UNASSIGNED,          // 235 M-k
    ED_UNASSIGNED,          // 236 M-l
    ED_UNASSIGNED,          // 237 M-m
    ED_UNASSIGNED,          // 238 M-n
    ED_UNASSIGNED,          // 239 M-o
    ED_UNASSIGNED,          // 240 M-p
    ED_UNASSIGNED,          // 241 M-q
    ED_UNASSIGNED,          // 242 M-r
    ED_UNASSIGNED,          // 243 M-s
    ED_UNASSIGNED,          // 244 M-t
    ED_UNASSIGNED,          // 245 M-u
    ED_UNASSIGNED,          // 246 M-v
    ED_UNASSIGNED,          // 247 M-w
    ED_UNASSIGNED,          // 248 M-x
    ED_UNASSIGNED,          // 249 M-y
    ED_UNASSIGNED,          // 250 M-z
    ED_UNASSIGNED,          // 251 M-{
    ED_UNASSIGNED,          // 252 M-|
    ED_UNASSIGNED,          // 253 M-}
    ED_UNASSIGNED,          // 254 M-~
    ED_UNASSIGNED,          // 255 M-^?
];

/// C: `L"..."` for an ASCII literal, as the crate carries wide strings.
/// `crate::fcns` has the same helper and keeps it private, being generated
/// output.
const fn wide<const N: usize>(s: &[u8; N]) -> [u32; N] {
    let mut out = [0u32; N];
    let mut i = 0;
    while i < N {
        out[i] = s[i] as u32;
        i += 1;
    }
    out
}

/// C: `wcsdup(L"_")` — vi's word-constituent set.
static WORDCHARS_VI: [u32; 1] = wide(b"_");
/// C: `wcsdup(L"*?_-.[]~=")` — emacs's.
static WORDCHARS_EMACS: [u32; 9] = wide(b"*?_-.[]~=");
/// C: `L"emacs"`, a static literal `map_get_editor` hands out.
static EDITOR_EMACS: [u32; 5] = wide(b"emacs");
/// C: `L"vi"`, likewise.
static EDITOR_VI: [u32; 2] = wide(b"vi");

// [spec:libedit:def:map.map-init-fn]
// [spec:libedit:sem:map.map-init-fn]
/// Initialize and allocate the maps. 0 on success, -1 if any allocation
/// failed, after tearing the rest back down.
pub(crate) fn map_init(el: &mut EditLine) -> i32 {
    // Step 1 is the `MAP_DEBUG` size assertion, which is the array type here.

    // Steps 2 and 3: the two heap maps. `el_calloc` returning NULL is a
    // `try_reserve` failure; the C's zero fill is `resize`.
    let mut alt: Vec<ElActionT> = Vec::new();
    if alt.try_reserve(N_KEYS).is_err() {
        return -1;
    }
    alt.resize(N_KEYS, 0);
    el.el_map.alt = alt;

    let mut key: Vec<ElActionT> = Vec::new();
    if key.try_reserve(N_KEYS).is_err() {
        map_end(el);
        return -1;
    }
    key.resize(N_KEYS, 0);
    el.el_map.key = key;

    // Step 4: borrow the three compiled-in tables. Nothing is copied and
    // nothing is owned.
    el.el_map.emacs = Some(&EL_MAP_EMACS);
    el.el_map.vic = Some(&EL_MAP_VI_COMMAND);
    el.el_map.vii = Some(&EL_MAP_VI_INSERT);

    // Step 5: the help table, the C's shallow `memcpy` of `el_func_help[]`.
    // Cloning a `Cow::Borrowed` is that shallow copy exactly: every one of
    // these `EL_NUM_FCNS` rows keeps pointing at the same static literals,
    // which is why `map_end` has nothing to free below that index.
    let mut help: Vec<ElBindingsT> = Vec::new();
    if help.try_reserve(EL_NUM_FCNS).is_err() {
        map_end(el);
        return -1;
    }
    help.extend(EL_FUNC_HELP.iter().map(|b| ElBindingsT {
        func: b.func,
        name: b.name.clone(),
        description: b.description.clone(),
    }));
    el.el_map.help = help;

    // Step 6. The copy length is `EL_NUM_FCNS` for both tables, which is
    // correct only because the numbering and the help table come from the
    // same `makelist` scan — `crate::fcns` has the tests that pin it.
    let mut func: Vec<ElFuncT> = Vec::new();
    if func.try_reserve(EL_NUM_FCNS).is_err() {
        map_end(el);
        return -1;
    }
    func.extend_from_slice(&EL_FUNC);
    el.el_map.func = func;

    // Step 7.
    el.el_map.nfunc = EL_NUM_FCNS;
    el.el_map.wordchars = None;

    // Step 8. `el.h` defines VIDEFAULT unconditionally, so the shipped
    // default editing mode is vi insert mode, matching `editline(7)`; this
    // call is also what first sets `type` and `current`.
    map_init_vi(el);
    0
}

// [spec:libedit:def:map.map-end-fn]
// [spec:libedit:sem:map.map-end-fn]
/// Free the space taken by the editor maps.
pub(crate) fn map_end(el: &mut EditLine) {
    // The C's order, kept because the rule enumerates it; the drops are
    // otherwise independent.
    el.el_map.alt = Vec::new();
    // ERR-modes-18, disposition fix: the C frees `wordchars` and leaves the
    // pointer dangling, so a second `map_end` double-frees it. Here it is
    // simply gone, which is what makes this function idempotent.
    el.el_map.wordchars = None;
    el.el_map.key = Vec::new();
    // Borrowed, never owned: nothing is freed for these three.
    el.el_map.emacs = None;
    el.el_map.vic = None;
    el.el_map.vii = None;
    // Step 4 in the C frees `help[nf].name`/`.description` for
    // `nf >= EL_NUM_FCNS` only, the index being the sole thing that
    // distinguishes the `wcsdup`ed rows from the borrowed literals. `Cow`
    // carries that distinction in the value, so dropping the vector frees
    // exactly the owned ones.
    el.el_map.help = Vec::new();
    el.el_map.func = Vec::new();
    // Not reset, as in the C: `nfunc`, `type` and `current` keep their old
    // values. Nothing reads past the (now empty) tables, since every scan is
    // bounded by the table it walks as well as by `nfunc`.
}

// [spec:libedit:def:map.map-init-nls-fn]
// [spec:libedit:sem:map.map-init-nls-fn]
/// Bind every printable high key to self-insert.
fn map_init_nls(el: &mut EditLine) {
    // Only the normal map, whatever the mode. The test is `iswprint` in the
    // process's LC_CTYPE locale applied to the *integer* 128..=255 read as a
    // wide character (U+0080..U+00FF), not to a byte of some multibyte
    // encoding: in the C locale nothing changes, in a UTF-8 locale the
    // printable part of U+00A0..U+00FF becomes ED_INSERT.
    //
    // This runs after `map_init_meta`, so in emacs mode it overwrites those
    // direct 8-bit meta bindings the default table supplied that sit at a
    // printable index; those commands stay reachable as the ESC-prefixed
    // keymacros `map_init_meta` just made. Indices 128..=159 are left alone,
    // the C1 controls not being `iswprint`, so they keep the direct 8-bit
    // binding on top of the ESC one. It is also what makes a non-ASCII
    // keystroke self-inserting rather than unassigned, which `search`'s
    // incremental search dispatches through (see the note on ERR-modes-32
    // there).
    let cs = locale::charset();
    for i in 0o200..=0o377usize {
        if locale::iswprint(cs, i as u32) {
            el.el_map.key[i] = ED_INSERT;
        }
    }
}

// [spec:libedit:def:map.map-init-meta-fn]
// [spec:libedit:sem:map.map-init-meta-fn]
/// Bind the meta keys to the matching `ESC`-prefixed sequences.
fn map_init_meta(el: &mut EditLine) {
    // Step 1: which map to work on, and which character is the meta prefix.
    // The C expresses "not found" as the loop counter reaching 0400, so an
    // EM_META_NEXT at index 255 counts as found.
    let mut map = ElMapCurrent::Key;
    let mut i = 0usize;
    while i <= 0o377 && el.el_map.key[i] != EM_META_NEXT {
        i += 1;
    }
    if i > 0o377 {
        i = 0;
        while i <= 0o377 && el.el_map.alt[i] != EM_META_NEXT {
            i += 1;
        }
        if i > 0o377 {
            i = 0o33;
            if el.el_map.r#type == MAP_VI {
                map = ElMapCurrent::Alt;
            }
        } else {
            map = ElMapCurrent::Alt;
        }
    }

    // Step 2. `buf[2]` is the C's terminator, which the slice length carries;
    // `buf[1]` is written only inside the loop, and the C leaves it
    // uninitialised when nothing qualifies — harmless there because
    // `keymacro_add` is then never called, and moot here.
    let mut buf = [0u32; 2];
    buf[0] = i as u32;

    // Step 3.
    for j in 0o200..=0o377usize {
        let action = map_slice(el, map)[j];
        // The C's `switch` with three no-op cases and a `default`.
        if matches!(action, ED_INSERT | ED_UNASSIGNED | ED_SEQUENCE_LEAD_IN) {
            continue;
        }
        buf[1] = (j & 0o177) as u32;
        let val = keymacro_map_cmd(el, i32::from(action));
        keymacro_add(el, &buf, &val, XK_CMD);
    }

    // Step 4, unconditional even when step 3 added nothing.
    let prefix = buf[0] as usize;
    map_slice_mut(el, map)[prefix] = ED_SEQUENCE_LEAD_IN;
}

// [spec:libedit:def:map.map-init-vi-fn]
// [spec:libedit:sem:map.map-init-vi-fn]
/// Install the vi bindings and make them current.
pub(crate) fn map_init_vi(el: &mut EditLine) {
    // The C reads `el_map.vii`/`vic` and the two heap maps unguarded, so
    // calling this after `map_end` dereferences NULL (ERR-modes-18). Defined
    // here as doing nothing at all, which is unobservable in the shipped
    // flow: `map_end` runs only from `el_end` and from `map_init`'s failure
    // path, neither of which switches mode afterwards.
    let (Some(vii), Some(vic)) = (el.el_map.vii, el.el_map.vic) else {
        return;
    };
    if el.el_map.key.len() != N_KEYS || el.el_map.alt.len() != N_KEYS {
        return;
    }

    // Steps 1 and 2: vi starts in *insert* mode, so `current` is the normal
    // map. `vi_command_mode` is what later moves it to `alt`.
    el.el_map.r#type = MAP_VI;
    el.el_map.current = ElMapCurrent::Key;

    // Step 3: every macro — user, arrow-key and meta — is gone.
    keymacro_reset(el);

    // Step 4. Unlike emacs mode, both maps are filled from tables.
    el.el_map.key.copy_from_slice(vii);
    el.el_map.alt.copy_from_slice(vic);

    // Step 5. For the shipped tables the net effect is only
    // `alt[27] = ED_SEQUENCE_LEAD_IN`: the insert map has no EM_META_NEXT, so
    // the command map is chosen, its high half is entirely
    // ED_UNASSIGNED/ED_SEQUENCE_LEAD_IN so no macro is created, and
    // `map_init_nls` is a no-op because the insert map's high half is already
    // ED_INSERT.
    map_init_meta(el);
    map_init_nls(el);

    // Steps 6 and 7.
    tty_bind_char(el, 1);
    terminal_bind_arrow(el);

    // Step 8. Unchecked in the C: OOM leaves the field NULL and this function
    // still cannot fail.
    el.el_map.wordchars = wcsdup(&WORDCHARS_VI);
}

// [spec:libedit:def:map.map-init-emacs-fn]
// [spec:libedit:sem:map.map-init-emacs-fn]
/// Install the emacs bindings and make them current.
pub(crate) fn map_init_emacs(el: &mut EditLine) {
    // As in `map_init_vi`: the C's post-`map_end` NULL dereference, defined
    // here as a no-op.
    let Some(emacs) = el.el_map.emacs else {
        return;
    };
    if el.el_map.key.len() != N_KEYS || el.el_map.alt.len() != N_KEYS {
        return;
    }

    // Steps 1 and 2. `current` points at the heap normal map, never at the
    // static `emacs` table — the invariant `chared`'s `current != emacs` test
    // rests on.
    el.el_map.r#type = MAP_EMACS;
    el.el_map.current = ElMapCurrent::Key;

    // Step 3.
    keymacro_reset(el);

    // Step 4. The alternate map is *blanked*, not filled from a table: emacs
    // mode has no second map.
    el.el_map.key.copy_from_slice(emacs);
    el.el_map.alt.fill(ED_UNASSIGNED);

    // Step 5. For the shipped emacs table this turns 34 meta bindings in
    // `key[128..=255]` into ESC-prefixed keymacros and makes `key[27]` a
    // lead-in, after which `map_init_nls` overwrites the printable part of
    // the high half with ED_INSERT.
    map_init_meta(el);
    map_init_nls(el);

    // Step 6: `^X ^X` -> EM_EXCHANGE_MARK. The lead-in marker for `^X` is not
    // written here — the static emacs table already has ED_SEQUENCE_LEAD_IN
    // at index 24.
    let buf = [CONTROL_X, CONTROL_X];
    let val = keymacro_map_cmd(el, i32::from(EM_EXCHANGE_MARK));
    keymacro_add(el, &buf, &val, XK_CMD);

    // Steps 7 and 8. With no default alt map in emacs mode `tty_bind_char`
    // touches only `key`.
    tty_bind_char(el, 1);
    terminal_bind_arrow(el);

    // Step 9, unchecked as above.
    el.el_map.wordchars = wcsdup(&WORDCHARS_EMACS);
}

// [spec:libedit:def:map.map-set-editor-fn]
// [spec:libedit:sem:map.map-set-editor-fn]
/// Switch to the named editor: 0 for `emacs` or `vi`, -1 for anything else.
pub fn map_set_editor(el: &mut EditLine, editor: &[u32]) -> i32 {
    // Exact and case-sensitive; no aliases, no abbreviations, no third mode.
    // A successful call is destructive — see the two initialisers.
    if wcs_eq_ascii(editor, b"emacs") {
        map_init_emacs(el);
        return 0;
    }
    if wcs_eq_ascii(editor, b"vi") {
        map_init_vi(el);
        return 0;
    }
    -1
}

// [spec:libedit:def:map.map-get-editor-fn]
// [spec:libedit:sem:map.map-get-editor-fn]
/// Report the current editor. The C's two answers are static wide literals,
/// so the out-parameter is a `&'static` one; its NULL check has no Rust
/// counterpart, a reference being non-null.
pub fn map_get_editor(el: &mut EditLine, editor: &mut &'static [u32]) -> i32 {
    match el.el_map.r#type {
        MAP_EMACS => {
            *editor = &EDITOR_EMACS;
            0
        }
        MAP_VI => {
            *editor = &EDITOR_VI;
            0
        }
        // Dead code (ERR-modes-71): `type` is only ever one of the two, and a
        // freshly built `EditLine` reads MAP_EMACS. Kept because the C's -1 is
        // reachable from any caller that hand-sets the field.
        _ => -1,
    }
}

// [spec:libedit:def:map.map-set-wordchars-fn]
// [spec:libedit:sem:map.map-set-wordchars-fn]
/// Replace the word-separator set with a copy of `wordchars`.
pub fn map_set_wordchars(el: &mut EditLine, wordchars: &[u32]) -> i32 {
    // ERR-modes-15, disposition define: the C frees the old set *before*
    // duplicating the argument, so handing back the pointer
    // `map_get_wordchars` lent out is a use-after-free. The copy is made
    // first here and the argument is a borrow, so the aliasing cannot arise.
    //
    // The C's `wcsdup` is unchecked: on failure the field is left NULL and
    // the function still reports success, with no way for a caller to tell
    // (ERR-core-api-30, disposition reproduce).
    el.el_map.wordchars = wcsdup(wordchars);
    0
}

// [spec:libedit:def:map.map-get-wordchars-fn]
// [spec:libedit:sem:map.map-get-wordchars-fn]
/// Hand out the word-separator set. The C lends its own buffer out; the
/// out-parameter mirrors the field's own type so the port can copy, and so
/// that the C's legitimately-NULL set stays distinguishable from an empty
/// one.
pub fn map_get_wordchars(el: &mut EditLine, wordchars: &mut Option<Vec<u32>>) -> i32 {
    // `None` is handed out and reported as success, as in the C: between
    // `map_init` and the mode init that follows it, and after a failed
    // `map_set_wordchars`, the field really is NULL and that means "the
    // built-in defaults are in use", not "empty" (ERR-core-api-30).
    *wordchars = el.el_map.wordchars.clone();
    0
}

// [spec:libedit:def:map.map-print-key-fn]
// [spec:libedit:sem:map.map-print-key-fn]
/// Print the function description for one key. `map` is the C's
/// `el_action_t *map`, always `el_map.key` or `el_map.alt`, so it is the
/// selector rather than a second alias of `el`.
fn map_print_key(el: &mut EditLine, map: ElMapCurrent, r#in: &[u32]) {
    // `in[0] == '\0' || in[1] == '\0'`, short-circuiting so the second read
    // never happens on an empty string.
    let c0 = r#in.first().copied().unwrap_or(0);
    let c1 = r#in.get(1).copied().unwrap_or(0);
    if c0 != 0 && c1 != 0 {
        // Two or more characters: walk the trie from that prefix, which
        // reports `Unbound extended key "..."` on a miss.
        keymacro_print(el, r#in);
        return;
    }

    // The empty separator means no surrounding quotes. An empty `in` renders
    // as `^@` and looks up `map[0]`, so `bind ""` reports the binding of NUL.
    let mut outbuf = [0u8; EL_BUFSIZ];
    keymacro__decode_str(upto_nul(r#in), &mut outbuf, EL_BUFSIZ, b"");

    // ERR-modes-31: the keymap index is `(unsigned char) *in`, so a first
    // character above U+00FF wraps modulo 256 onto an unrelated slot.
    let action = map_slice(el, map)[(c0 & 0xff) as usize];
    let name = el
        .el_map
        .help
        .iter()
        .take(el.el_map.nfunc)
        .find(|bp| bp.func == i32::from(action))
        .map(|bp| bp.name.to_vec());

    // ERR-modes-29: no matching help entry prints nothing at all here, where
    // `map_print_some_keys` aborts.
    if let Some(name) = name {
        let mut out = cstr(&outbuf).to_vec();
        out.extend_from_slice(b"\t->\t");
        out.extend_from_slice(&wcs_to_mb(&name));
        out.push(b'\n');
        write_outfile(el, &out);
    }
}

// [spec:libedit:def:map.map-print-some-keys-fn]
// [spec:libedit:sem:map.map-print-some-keys-fn]
/// Print the binding shared by the keys `first` through `last`.
fn map_print_some_keys(el: &mut EditLine, map: ElMapCurrent, first: u32, last: u32) {
    // The caller guarantees `first <= last`, both within 0..=255, and
    // `map[first] == map[last]`.
    let firstbuf = [first];
    let lastbuf = [last];
    let action = map_slice(el, map)[first as usize];

    if action == ED_UNASSIGNED {
        // Unassigned *ranges* are invisible in `bind` output; unassigned
        // single keys are reported.
        if first == last {
            let mut out = Vec::new();
            pad(&mut out, &decode(&firstbuf, STRQQ), 15);
            out.extend_from_slice(b"->  is undefined\n");
            write_outfile(el, &out);
        }
        return;
    }

    let name = el
        .el_map
        .help
        .iter()
        .take(el.el_map.nfunc)
        .find(|bp| bp.func == i32::from(action))
        .map(|bp| bp.name.to_vec());

    let Some(name) = name else {
        // ERR-modes-28, disposition needs decision. The C falls through to
        // `EL_ABORT`, i.e. `abort()`, and the rule directs the port not to
        // abort literally but to treat this as an internal invariant
        // violation — which is what this is: a keymap slot holding a command
        // number no help entry claims. Reachable, because `map_bind` stores
        // command numbers through an `unsigned char` cast, so the 160th
        // function added by `map_addfunc` truncates onto a number that may
        // name nothing.
        panic!("map_print_some_keys: key {first} is bound to unclaimed action {action}");
    };

    let mut out = Vec::new();
    if first == last {
        // "%-15s->  %ls\n"
        pad(&mut out, &decode(&firstbuf, STRQQ), 15);
    } else {
        // "%-4s to %-7s->  %ls\n"
        pad(&mut out, &decode(&firstbuf, STRQQ), 4);
        out.extend_from_slice(b" to ");
        pad(&mut out, &decode(&lastbuf, STRQQ), 7);
    }
    out.extend_from_slice(b"->  ");
    out.extend_from_slice(&wcs_to_mb(&name));
    out.push(b'\n');
    write_outfile(el, &out);
}

// [spec:libedit:def:map.map-print-all-keys-fn]
// [spec:libedit:sem:map.map-print-all-keys-fn]
/// Print the function description for all keys, both maps, the trie and the
/// arrow keys.
fn map_print_all_keys(el: &mut EditLine) {
    // All four headings are printed even when the section under them is
    // empty; the order and the exact strings are observable output.
    write_outfile(el, b"Standard key bindings\n");
    print_runs(el, ElMapCurrent::Key);

    // In emacs mode `alt` is uniformly ED_UNASSIGNED, so this is one run of
    // 256 keys that `map_print_some_keys` prints nothing for: the heading
    // appears with no entries under it.
    write_outfile(el, b"Alternative key bindings\n");
    print_runs(el, ElMapCurrent::Alt);

    write_outfile(el, b"Multi-character bindings\n");
    keymacro_print(el, &[]);
    write_outfile(el, b"Arrow key bindings\n");
    terminal_print_arrow(el, &[]);
}

/// The C's run-coalescing walk, written out twice in `map_print_all_keys`.
///
/// `i == prev` always compares equal, so a run has at least one element and
/// `i - 1 >= prev`; the trailing call flushes the final run.
fn print_runs(el: &mut EditLine, map: ElMapCurrent) {
    let mut prev = 0usize;
    for i in 0..N_KEYS {
        if map_slice(el, map)[prev] == map_slice(el, map)[i] {
            continue;
        }
        map_print_some_keys(el, map, prev as u32, (i - 1) as u32);
        prev = i;
    }
    map_print_some_keys(el, map, prev as u32, (N_KEYS - 1) as u32);
}

// [spec:libedit:def:map.map-bind-fn]
// [spec:libedit:sem:map.map-bind-fn]
/// The `bind` builtin: add, remove, change or show bindings. `argc` is the
/// C's — reassigned on entry and ignored — and the C's NULL terminator on
/// `argv` is the slice length here.
pub fn map_bind(el: &mut EditLine, argc: i32, argv: &[&[u32]]) -> i32 {
    // ERR-modes-27. The C's first statement of the argument loop is
    // `argc = 1`, so the caller's count is dead and iteration runs to the
    // first NULL element instead — which is why `el_wset(EL_BIND, ...)`
    // reads past its 20-element array when 19 arguments were passed and it
    // omitted the terminator. A slice carries its own length and cannot
    // express that over-read: the defect bites at the ABI shim, which must
    // build this slice from the terminator, not from the caller's count.
    let _ = argc;

    // Step 1: the C's `argv == NULL`. An empty slice is the closest thing —
    // the C would read `argv[0]`, the command name every diagnostic prints,
    // off the end.
    let Some(prog) = argv.first().copied() else {
        return -1;
    };

    // Step 2.
    let mut map = ElMapCurrent::Key;
    let mut ntype = XK_CMD;
    let mut key = false;
    let mut rem = false;

    // Step 3: the option scan. Only `argv[i][1]` is examined, so clustered
    // flags do not work — `-ar` is read as `-a` and the rest is discarded
    // (ERR-modes-30) — and the first non-`-` argument ends the scan, so a
    // later `-x` is data.
    let mut i = 1usize;
    while i < argv.len() {
        let p = argv[i];
        if p.first().copied().unwrap_or(0) != u32::from(b'-') {
            break;
        }
        match u8::try_from(p.get(1).copied().unwrap_or(0)) {
            Ok(b'a') => map = ElMapCurrent::Alt,
            Ok(b's') => ntype = XK_STR,
            Ok(b'k') => key = true,
            Ok(b'r') => rem = true,
            Ok(b'v') => {
                map_init_vi(el);
                return 0;
            }
            Ok(b'e') => {
                map_init_emacs(el);
                return 0;
            }
            Ok(b'l') => {
                let mut out = Vec::new();
                for bp in el.el_map.help.iter().take(el.el_map.nfunc) {
                    // "%ls\n\t%ls\n"
                    out.extend_from_slice(&wcs_to_mb(&bp.name));
                    out.extend_from_slice(b"\n\t");
                    out.extend_from_slice(&wcs_to_mb(&bp.description));
                    out.push(b'\n');
                }
                write_outfile(el, &out);
                return 0;
            }
            // Anything else, a bare `-` included (where the C reads the
            // terminating NUL): diagnose and *continue* the scan. The
            // malformed flag is consumed and skipped, not an error return.
            other => {
                let c = other.map_or_else(|_| p[1], u32::from);
                let mut out = wcs_to_mb(prog);
                out.extend_from_slice(b": Invalid switch `");
                out.extend_from_slice(&wcs_to_mb(&[c]));
                out.extend_from_slice(b"'.\n");
                write_errfile(el, &out);
            }
        }
        i += 1;
    }

    // Step 4: no key argument at all. A bare `bind`, or `bind -a`, dumps the
    // whole binding state.
    if i >= argv.len() {
        map_print_all_keys(el);
        return 0;
    }

    // Step 5: the key argument.
    let arg = argv[i];
    i += 1;
    let decoded: Vec<u32>;
    let r#in: &[u32] = if key {
        // A function-key name, passed through verbatim.
        upto_nul(arg)
    } else {
        // ERR-modes-13, disposition define: the C decodes into a 1024-wide
        // character stack buffer and `parse__string` bounds nothing, so a
        // longer argument smashes the stack. The buffer is sized by the input
        // here — the decode never grows a string — which removes the overflow
        // without truncating anything the C would have accepted.
        let mut buf = vec![0u32; arg.len() + 1];
        let Some(n) = parse__string(&mut buf, arg).map(<[u32]>::len) else {
            let mut out = wcs_to_mb(prog);
            out.extend_from_slice(b": Invalid \\ or ^ in instring.\n");
            write_errfile(el, &out);
            return -1;
        };
        buf.truncate(n);
        decoded = buf;
        // Everything downstream reads `in` as a C string, so an embedded NUL
        // — `^@`, `\0` and `\U+0000` all decode to one — ends it.
        upto_nul(&decoded)
    };

    // `(unsigned char) *in` throughout, and `in[1]` for "is this a sequence".
    // ERR-modes-12, disposition define: the C reads `in[1]` even when `in[0]`
    // is the terminator, so `bind -r ""` reads an uninitialised stack slot;
    // an absent character is the terminator here, which is the rule's
    // "treat an empty sequence as the single-element case".
    let in0 = r#in.first().copied().unwrap_or(0);
    let in1 = r#in.get(1).copied().unwrap_or(0);
    let idx = (in0 & 0xff) as usize;

    // Step 6: removal.
    if rem {
        if key {
            // ERR-modes-25, disposition reproduce: -1 even when the clear
            // succeeded, and `el_parse` negates it into a positive return.
            terminal_clear_arrow(el, r#in);
            return -1;
        }
        if in1 != 0 {
            // The lead-in marker left in the keymap is *not* cleared, which
            // is how the keymap and the trie fall out of step (ERR-input-03).
            keymacro_delete(el, r#in);
        } else if map_slice(el, map)[idx] == ED_SEQUENCE_LEAD_IN {
            keymacro_delete(el, r#in);
        } else {
            map_slice_mut(el, map)[idx] = ED_UNASSIGNED;
        }
        return 0;
    }

    // Step 7: query.
    if i >= argv.len() {
        if key {
            terminal_print_arrow(el, r#in);
        } else {
            map_print_key(el, map, r#in);
        }
        return 0;
    }

    // Step 8: extra arguments past the value are silently ignored — the
    // arity check is inside `#ifdef notyet` and is not compiled, so
    // `bind ^A ed-move-to-beg junk extra` succeeds (ERR-modes-71).

    // Step 9: install.
    let value = argv[i];
    match ntype {
        XK_STR => {
            let mut buf = vec![0u32; value.len() + 1];
            let Some(n) = parse__string(&mut buf, value).map(<[u32]>::len) else {
                let mut out = wcs_to_mb(prog);
                out.extend_from_slice(b": Invalid \\ or ^ in outstring.\n");
                write_errfile(el, &out);
                return -1;
            };
            buf.truncate(n);
            let out = upto_nul(&buf).to_vec();

            // ERR-modes-14, disposition define: in the C `keymacro_map_str`
            // only parks the pointer in the shared scratch union, and
            // `terminal_set_arrow` stores that raw pointer, so the function
            // key ends up holding a pointer to this function's stack buffer.
            // `KeymacroValueT::Str` owns its buffer, so the copy is real and
            // nothing dangles.
            let val = keymacro_map_str(el, &out);
            if key {
                terminal_set_arrow(el, r#in, val, XK_STR);
            } else {
                keymacro_add(el, r#in, &val, XK_STR);
            }
            // ERR-modes-26, disposition reproduce: unconditional, `-k`
            // included, where `in` is a function-key *name* — so
            // `bind -k -s up x` sets `map['u'] = ED_SEQUENCE_LEAD_IN` and
            // corrupts the binding of the letter `u`.
            map_slice_mut(el, map)[idx] = ED_SEQUENCE_LEAD_IN;
        }

        XK_CMD => {
            let cmd = parse_cmd(el, value);
            if cmd == -1 {
                let mut out = wcs_to_mb(prog);
                out.extend_from_slice(b": Invalid command `");
                out.extend_from_slice(&wcs_to_mb(value));
                out.extend_from_slice(b"'.\n");
                write_errfile(el, &out);
                return -1;
            }
            if key {
                // The result is discarded: an unknown function-key name is
                // silently ignored, and the keymap is not touched here.
                let val = keymacro_map_cmd(el, cmd);
                terminal_set_arrow(el, r#in, val, XK_CMD);
            } else if in1 != 0 {
                // `keymacro_add` rejects an empty sequence and rejects
                // binding one to ED_SEQUENCE_LEAD_IN, printing and returning
                // without doing anything — the keymap write still happens.
                let val = keymacro_map_cmd(el, cmd);
                keymacro_add(el, r#in, &val, XK_CMD);
                map_slice_mut(el, map)[idx] = ED_SEQUENCE_LEAD_IN;
            } else {
                keymacro_clear(el, map, r#in);
                // ERR-modes-28, disposition reproduce: `(el_action_t)cmd`
                // truncates modulo 256, so a command number above 255 lands
                // on an unrelated one — or on none, which is what makes
                // `map_print_some_keys` reachable.
                map_slice_mut(el, map)[idx] = cmd as ElActionT;
            }
        }

        // Dead: `ntype` is only ever XK_CMD or XK_STR, which is why the C
        // marks its `EL_ABORT` here `coverity[dead_error_begin]`.
        _ => panic!("map_bind: bad XK_ type {ntype}"),
    }

    // Step 10.
    0
}

// [spec:libedit:def:map.map-addfunc-fn]
// [spec:libedit:sem:map.map-addfunc-fn]
/// Append a user-defined editor function and its help entry.
pub fn map_addfunc(el: &mut EditLine, name: &[u32], help: &[u32], func: ElFuncT) -> i32 {
    // The C's three NULL checks have no counterpart: a reference is non-null
    // and `ElFuncT` is not nullable. There is no other validation — a name
    // identical to an existing one is accepted, and since `parse_cmd` returns
    // the first match the later duplicate is simply unreachable by name.

    // The C grows `func` first and `help` second, and returns -1 between them
    // if the second fails — leaving `func` one slot longer than `nfunc`
    // claims. Reserving without pushing is that state exactly: spare
    // capacity, an unchanged length, and an unchanged `nfunc`.
    if el.el_map.func.try_reserve(1).is_err() {
        return -1;
    }
    if el.el_map.help.try_reserve(1).is_err() {
        return -1;
    }

    // The new entry's index *is* its command number, continuing the generated
    // numbering: EL_NUM_FCNS, EL_NUM_FCNS+1, ... `el_action_t` is one byte,
    // so with EL_NUM_FCNS == 96 the 160th function added is the first whose
    // number a keymap slot cannot hold; `map_bind` stores it truncated
    // (ERR-modes-28), and the dispatcher independently ignores any number
    // >= `nfunc`.
    let nf = el.el_map.nfunc;

    // The function pointer is stored raw; the map does not own it.
    el.el_map.func.push(func);

    // ERR-modes-11, disposition define: the C checks neither `wcsdup`, so on
    // OOM the row keeps a NULL name or description while `nfunc` has already
    // been bumped, and `parse_cmd`/`bind -l` then hand NULL to `wcscmp`/`%ls`.
    // The rule prefers failing the call, and an owned `Cow` cannot be absent,
    // so the copies are made before anything is published.
    let (Some(name), Some(description)) = (wcsdup(name), wcsdup(help)) else {
        el.el_map.func.pop();
        return -1;
    };
    el.el_map.help.push(ElBindingsT {
        func: nf as i32,
        name: Cow::Owned(name),
        description: Cow::Owned(description),
    });

    el.el_map.nfunc += 1;
    0
}

// ---------------------------------------------------------------------------
// Helpers with no C counterpart: the C reaches these through pointer
// arithmetic, `stdio` and `wchar.h`.
// ---------------------------------------------------------------------------

/// The C's `el_action_t *map` argument, resolved against the `EditLine`.
fn map_slice(el: &EditLine, map: ElMapCurrent) -> &[ElActionT] {
    match map {
        ElMapCurrent::Key => &el.el_map.key,
        ElMapCurrent::Alt => &el.el_map.alt,
    }
}

/// The mutable form of [`map_slice`].
fn map_slice_mut(el: &mut EditLine, map: ElMapCurrent) -> &mut [ElActionT] {
    match map {
        ElMapCurrent::Key => &mut el.el_map.key,
        ElMapCurrent::Alt => &mut el.el_map.alt,
    }
}

/// C: `wcsdup` — an owned copy of a wide string, `None` for its NULL return.
///
/// The copy stops at the first NUL, as `wcsdup` does.
fn wcsdup(s: &[u32]) -> Option<Vec<u32>> {
    let s = upto_nul(s);
    let mut out = Vec::new();
    out.try_reserve(s.len()).ok()?;
    out.extend_from_slice(s);
    Some(out)
}

/// The prefix of a wide string before its first NUL: what every C callee sees
/// of a `wchar_t *` that the port carries as a slice.
fn upto_nul(s: &[u32]) -> &[u32] {
    match s.iter().position(|&c| c == 0) {
        Some(i) => &s[..i],
        None => s,
    }
}

/// The same for the narrow buffers `keymacro__decode_str` writes, which it
/// always NUL-terminates and which the C then passes to `%s`.
fn cstr(buf: &[u8]) -> &[u8] {
    match buf.iter().position(|&c| c == 0) {
        Some(i) => &buf[..i],
        None => buf,
    }
}

/// C: `keymacro__decode_str(str, buf, sizeof buf, sep)` followed by `%s` of
/// the result, which is what all six call sites in `map.c` do.
fn decode(s: &[u32], sep: &[u8]) -> Vec<u8> {
    let mut buf = [0u8; EL_BUFSIZ];
    keymacro__decode_str(upto_nul(s), &mut buf, EL_BUFSIZ, sep);
    cstr(&buf).to_vec()
}

/// C: `printf`'s `%-Ns` — the bytes, then spaces up to `width`. Never
/// truncates, and the width counts bytes of the rendered form, which is
/// observable output.
fn pad(out: &mut Vec<u8>, s: &[u8], width: usize) {
    out.extend_from_slice(s);
    for _ in s.len()..width {
        out.push(b' ');
    }
}

/// C: `printf`'s `%ls`/`%lc` — the wide string converted with `wcrtomb` in
/// the current locale. Conversion stops at the first character the locale
/// cannot encode, as `fprintf` does when `wcrtomb` reports EILSEQ.
fn wcs_to_mb(s: &[u32]) -> Vec<u8> {
    let cs = locale::charset();
    let mut out = Vec::new();
    let mut buf = [0u8; locale::MB_LEN_MAX];
    for &c in upto_nul(s) {
        match locale::wcrtomb(cs, c, &mut buf) {
            Some(n) => out.extend_from_slice(&buf[..n]),
            None => break,
        }
    }
    out
}

/// C: `wcscmp(s, L"...") == 0` against an ASCII literal, so `s` ends at its
/// first NUL.
fn wcs_eq_ascii(s: &[u32], lit: &[u8]) -> bool {
    let s = upto_nul(s);
    s.len() == lit.len() && s.iter().zip(lit).all(|(&c, &b)| c == u32::from(b))
}

/// C: `fprintf(el->el_outfile, ...)` for an already-formatted byte string.
///
/// The stream is a caller-owned `FILE *` the port cannot write through, so
/// this goes to the matching descriptor, which the `EditLine` carries for
/// exactly this reason (`def:el.editline`). Errors are discarded, as the C
/// discards `fprintf`'s result.
fn write_outfile(el: &EditLine, bytes: &[u8]) {
    write_fd(el.el_outfd, bytes);
}

/// C: `fprintf(el->el_errfile, ...)`, likewise.
fn write_errfile(el: &EditLine, bytes: &[u8]) {
    write_fd(el.el_errfd, bytes);
}

fn write_fd(fd: i32, bytes: &[u8]) {
    if fd < 0 {
        return;
    }
    // SAFETY: the descriptor is the application's and stays open for the life
    // of the `EditLine`; `ManuallyDrop` is what keeps this borrow from
    // closing it, which libedit never does.
    let mut out = ManuallyDrop::new(unsafe { std::fs::File::from_raw_fd(fd) });
    let _ = out.write_all(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    // The facts other rules depend on, per `sem:map.map-init-fn`. The
    // transcription is by hand from a table whose own index comments are
    // wrong from `M-_` onward (ERR-modes-70), so these are worth pinning.

    #[test]
    fn the_emacs_table_matches_the_rule() {
        assert_eq!(EL_MAP_EMACS[24], ED_SEQUENCE_LEAD_IN, "^X");
        assert_eq!(EL_MAP_EMACS[27], EM_META_NEXT, "ESC");
        let meta = EL_MAP_EMACS[128..]
            .iter()
            .filter(|&&a| !matches!(a, ED_INSERT | ED_UNASSIGNED | ED_SEQUENCE_LEAD_IN))
            .count();
        assert_eq!(meta, 34, "real meta bindings in 128..=255");
    }

    #[test]
    fn the_vi_insert_table_matches_the_rule() {
        assert!(!EL_MAP_VI_INSERT.contains(&EM_META_NEXT));
        assert!(!EL_MAP_VI_INSERT.contains(&ED_SEQUENCE_LEAD_IN));
        assert_eq!(EL_MAP_VI_INSERT[27], VI_COMMAND_MODE);
        assert!(EL_MAP_VI_INSERT[128..].iter().all(|&a| a == ED_INSERT));
    }

    #[test]
    fn the_vi_command_table_matches_the_rule() {
        assert_eq!(EL_MAP_VI_COMMAND[27], EM_META_NEXT);
        let high = &EL_MAP_VI_COMMAND[128..];
        assert_eq!(high.iter().filter(|&&a| a == ED_UNASSIGNED).count(), 126);
        assert_eq!(
            high.iter().filter(|&&a| a == ED_SEQUENCE_LEAD_IN).count(),
            2
        );
    }

    /// An editor whose maps are allocated and filled the way `el_init` leaves
    /// them, with the three descriptors closed off. A `calloc`ed `EditLine`
    /// carries 0 in all three, which is the process's standard input, and
    /// several of the functions below print on their error paths.
    fn mapped_editline() -> EditLine {
        let mut el = crate::el::blank_editline();
        el.el_infd = -1;
        el.el_outfd = -1;
        el.el_errfd = -1;
        assert_eq!(map_init(&mut el), 0);
        el
    }

    fn w(s: &str) -> Vec<u32> {
        s.chars().map(u32::from).collect()
    }

    /// `type` is the sole source of truth for the mode *name*, while the
    /// bindings live in the heap tables — so a caller that hand-edits a
    /// binding still reads back the mode it last selected. That asymmetry is
    /// the whole content of the rule, and no differential can see it, because
    /// `el_get(EL_EDITOR)` and the keymap are separate observations.
    ///
    /// The shipped default is vi: `el.h` defines VIDEFAULT, so `map_init`
    /// ends in `map_init_vi`.
    // [spec:libedit:sem:map.map-get-editor-fn/test]
    #[test]
    fn the_editor_name_comes_from_the_mode_tag_and_not_from_the_bindings() {
        let mut el = mapped_editline();
        let mut name: &[u32] = &[];
        assert_eq!(map_get_editor(&mut el, &mut name), 0);
        assert_eq!(name, EDITOR_VI, "map_init ends in map_init_vi");

        assert_eq!(map_set_editor(&mut el, &w("emacs")), 0);
        assert_eq!(map_get_editor(&mut el, &mut name), 0);
        assert_eq!(name, EDITOR_EMACS);

        // Rebinding every slot in the live map does not move the name.
        el.el_map.key.fill(ED_UNASSIGNED);
        assert_eq!(map_get_editor(&mut el, &mut name), 0);
        assert_eq!(name, EDITOR_EMACS);

        // The switch's fall-through, which ERR-modes-71 records as dead: only
        // a caller that hand-sets `type` reaches it, and the out-parameter is
        // left holding whatever it held.
        el.el_map.r#type = 42;
        assert_eq!(map_get_editor(&mut el, &mut name), -1);
        assert_eq!(name, EDITOR_EMACS, "the out-parameter is not touched");
    }

    /// The set is an owned copy that stops at the first NUL, and the getter
    /// hands back what is stored — including the legitimate "nothing", which
    /// is the state between `map_init` and the mode init that follows it and
    /// means "the built-in defaults are in use" rather than "empty"
    /// (ERR-core-api-30). Both report success either way, so the report is no
    /// evidence at all.
    ///
    /// The C's aliasing hazard has no counterpart here and that is the point
    /// of ERR-modes-15: the C frees the old set before duplicating the
    /// argument, so feeding `map_get_wordchars`'s own pointer back in is a
    /// use-after-free. Round-tripping the value is safe here by construction.
    // [spec:libedit:sem:map.map-set-wordchars-fn/test]
    // [spec:libedit:sem:map.map-get-wordchars-fn/test]
    #[test]
    fn the_word_character_set_round_trips_and_may_legitimately_be_absent() {
        let mut el = crate::el::blank_editline();
        let mut got: Option<Vec<u32>> = Some(w("stale"));
        assert_eq!(map_get_wordchars(&mut el, &mut got), 0);
        assert_eq!(got, None, "absent is reported as success, not as empty");

        let mut el = mapped_editline();
        assert_eq!(map_get_wordchars(&mut el, &mut got), 0);
        assert_eq!(got.as_deref(), Some(&WORDCHARS_VI[..]), "vi's default");

        assert_eq!(map_set_wordchars(&mut el, &w("abc")), 0);
        assert_eq!(map_get_wordchars(&mut el, &mut got), 0);
        assert_eq!(got.as_deref(), Some(&w("abc")[..]));

        // `wcsdup` stops at the terminator, so an embedded NUL truncates the
        // stored set rather than being carried into it.
        assert_eq!(
            map_set_wordchars(&mut el, &[b'a'.into(), 0, b'b'.into()]),
            0
        );
        assert_eq!(map_get_wordchars(&mut el, &mut got), 0);
        assert_eq!(got.as_deref(), Some(&w("a")[..]));

        // A mode switch reinstalls the mode default over whatever was set.
        assert_eq!(map_set_editor(&mut el, &w("emacs")), 0);
        assert_eq!(map_get_wordchars(&mut el, &mut got), 0);
        assert_eq!(got.as_deref(), Some(&WORDCHARS_EMACS[..]));
    }

    // Two distinguishable `ElFuncT` values. The map stores the pointer raw
    // and never calls it here; what matters is that the two rows are separate
    // rows.
    unsafe extern "C" fn fn_a(_el: *mut EditLine, _c: u32) -> ElActionT {
        1
    }
    unsafe extern "C" fn fn_b(_el: *mut EditLine, _c: u32) -> ElActionT {
        2
    }

    /// The new entry's index *is* its command number, continuing the
    /// generated numbering, and that is what `bind` stores in a keymap slot.
    /// There is no uniqueness check: a name that collides with an existing
    /// one is accepted and the later row becomes permanently unreachable by
    /// name, because `parse_cmd` returns the first match. That is not an
    /// error and nothing reports it — the only way to see it is to add the
    /// duplicate and ask `parse_cmd`.
    // [spec:libedit:sem:map.map-addfunc-fn/test]
    #[test]
    fn an_added_function_is_numbered_by_its_index_and_may_be_shadowed() {
        let mut el = mapped_editline();
        let base = el.el_map.nfunc;
        assert_eq!(base, EL_NUM_FCNS);

        assert_eq!(map_addfunc(&mut el, &w("mine"), &w("help"), fn_a), 0);
        assert_eq!(el.el_map.nfunc, base + 1);
        assert_eq!(el.el_map.help[base].func, base as i32);
        assert_eq!(el.el_map.help[base].description, w("help"));
        assert_eq!(
            parse_cmd(&mut el, &w("mine")),
            base as i32,
            "the index is the command number bind will store"
        );

        // A second row under the same name: accepted, numbered normally, and
        // unreachable through the only lookup there is.
        assert_eq!(map_addfunc(&mut el, &w("mine"), &w("other"), fn_b), 0);
        assert_eq!(el.el_map.nfunc, base + 2);
        assert_eq!(el.el_map.help[base + 1].func, (base + 1) as i32);
        assert_eq!(
            parse_cmd(&mut el, &w("mine")),
            base as i32,
            "first match wins, so the duplicate is unreachable by name"
        );

        // The name and help are copied, not borrowed: the caller's storage is
        // not retained, and the copies stop at the first NUL as `wcsdup` does.
        assert_eq!(
            map_addfunc(&mut el, &[b'x'.into(), 0, b'y'.into()], &w("h"), fn_a),
            0
        );
        assert_eq!(parse_cmd(&mut el, &w("x")), (base + 2) as i32);
        assert_eq!(parse_cmd(&mut el, &w("xy")), -1);

        // `nfunc` is what bounds the lookup, so a row above it is invisible
        // even though the vector still holds it.
        el.el_map.nfunc = base;
        assert_eq!(parse_cmd(&mut el, &w("mine")), -1);
    }
}
