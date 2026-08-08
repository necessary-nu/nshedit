#![cfg_attr(rustfmt, rustfmt_skip)]

// Three columns, in ncurses' own order and generated from its `include/Caps`:
// the long name, the terminfo capname, and the termcap two-letter code. The
// third is the one `term` 1.2.1 dropped, and libedit needs it — `settc`,
// `echotc` and `EL_SETTC` all take a name a user typed, at a prompt or in
// `.editrc`, and those are termcap codes.
//
// Every capability has a termcap code — measured: 0 of 497 rows in `Caps` 6.5
// carry `-` in that column. The codes are not unique, though; see
// `capname_for_termcap`.

/// Long boolean capability names in ncurses table order.
pub static BOOL_LONG_NAMES: &[&str] = &["auto_left_margin",
                                   "auto_right_margin",
                                   "no_esc_ctlc",
                                   "ceol_standout_glitch",
                                   "eat_newline_glitch",
                                   "erase_overstrike",
                                   "generic_type",
                                   "hard_copy",
                                   "has_meta_key",
                                   "has_status_line",
                                   "insert_null_glitch",
                                   "memory_above",
                                   "memory_below",
                                   "move_insert_mode",
                                   "move_standout_mode",
                                   "over_strike",
                                   "status_line_esc_ok",
                                   "dest_tabs_magic_smso",
                                   "tilde_glitch",
                                   "transparent_underline",
                                   "xon_xoff",
                                   "needs_xon_xoff",
                                   "prtr_silent",
                                   "hard_cursor",
                                   "non_rev_rmcup",
                                   "no_pad_char",
                                   "non_dest_scroll_region",
                                   "can_change",
                                   "back_color_erase",
                                   "hue_lightness_saturation",
                                   "col_addr_glitch",
                                   "cr_cancels_micro_mode",
                                   "has_print_wheel",
                                   "row_addr_glitch",
                                   "semi_auto_right_margin",
                                   "cpi_changes_res",
                                   "lpi_changes_res",
                                   "backspaces_with_bs",
                                   "crt_no_scrolling",
                                   "no_correctly_working_cr",
                                   "gnu_has_meta_key",
                                   "linefeed_is_newline",
                                   "has_hardware_tabs",
                                   "return_does_clr_eol"];

/// Short terminfo boolean capability names in ncurses table order.
pub static BOOL_NAMES: &[&str] =
    &["bw", "am", "xsb", "xhp", "xenl", "eo", "gn", "hc", "km", "hs", "in", "db", "da", "mir",
      "msgr", "os", "eslok", "xt", "hz", "ul", "xon", "nxon", "mc5i", "chts", "nrrmc", "npc",
      "ndscr", "ccc", "bce", "hls", "xhpa", "crxm", "daisy", "xvpa", "sam", "cpix", "lpix",
      "OTbs", "OTns", "OTnc", "OTMT", "OTNL", "OTpt", "OTxr"];

/// Long numeric capability names in ncurses table order.
pub static NUMBER_LONG_NAMES: &[&str] = &["columns",
                                  "init_tabs",
                                  "lines",
                                  "lines_of_memory",
                                  "magic_cookie_glitch",
                                  "padding_baud_rate",
                                  "virtual_terminal",
                                  "width_status_line",
                                  "num_labels",
                                  "label_height",
                                  "label_width",
                                  "max_attributes",
                                  "maximum_windows",
                                  "max_colors",
                                  "max_pairs",
                                  "no_color_video",
                                  "buffer_capacity",
                                  "dot_vert_spacing",
                                  "dot_horz_spacing",
                                  "max_micro_address",
                                  "max_micro_jump",
                                  "micro_col_size",
                                  "micro_line_size",
                                  "number_of_pins",
                                  "output_res_char",
                                  "output_res_line",
                                  "output_res_horz_inch",
                                  "output_res_vert_inch",
                                  "print_rate",
                                  "wide_char_size",
                                  "buttons",
                                  "bit_image_entwining",
                                  "bit_image_type",
                                  "magic_cookie_glitch_ul",
                                  "carriage_return_delay",
                                  "new_line_delay",
                                  "backspace_delay",
                                  "horizontal_tab_delay",
                                  "number_of_function_keys"];

/// Short terminfo numeric capability names in ncurses table order.
pub static NUMBER_NAMES: &[&str] =
    &["cols", "it", "lines", "lm", "xmc", "pb", "vt", "wsl", "nlab", "lh", "lw", "ma", "wnum",
      "colors", "pairs", "ncv", "bufsz", "spinv", "spinh", "maddr", "mjump", "mcs", "mls",
      "npins", "orc", "orl", "orhi", "orvi", "cps", "widcs", "btns", "bitwin", "bitype", "OTug",
      "OTdC", "OTdN", "OTdB", "OTdT", "OTkn"];

/// Long string capability names in ncurses table order.
pub static STRING_LONG_NAMES: &[&str] = &["back_tab",
                                     "bell",
                                     "carriage_return",
                                     "change_scroll_region",
                                     "clear_all_tabs",
                                     "clear_screen",
                                     "clr_eol",
                                     "clr_eos",
                                     "column_address",
                                     "command_character",
                                     "cursor_address",
                                     "cursor_down",
                                     "cursor_home",
                                     "cursor_invisible",
                                     "cursor_left",
                                     "cursor_mem_address",
                                     "cursor_normal",
                                     "cursor_right",
                                     "cursor_to_ll",
                                     "cursor_up",
                                     "cursor_visible",
                                     "delete_character",
                                     "delete_line",
                                     "dis_status_line",
                                     "down_half_line",
                                     "enter_alt_charset_mode",
                                     "enter_blink_mode",
                                     "enter_bold_mode",
                                     "enter_ca_mode",
                                     "enter_delete_mode",
                                     "enter_dim_mode",
                                     "enter_insert_mode",
                                     "enter_secure_mode",
                                     "enter_protected_mode",
                                     "enter_reverse_mode",
                                     "enter_standout_mode",
                                     "enter_underline_mode",
                                     "erase_chars",
                                     "exit_alt_charset_mode",
                                     "exit_attribute_mode",
                                     "exit_ca_mode",
                                     "exit_delete_mode",
                                     "exit_insert_mode",
                                     "exit_standout_mode",
                                     "exit_underline_mode",
                                     "flash_screen",
                                     "form_feed",
                                     "from_status_line",
                                     "init_1string",
                                     "init_2string",
                                     "init_3string",
                                     "init_file",
                                     "insert_character",
                                     "insert_line",
                                     "insert_padding",
                                     "key_backspace",
                                     "key_catab",
                                     "key_clear",
                                     "key_ctab",
                                     "key_dc",
                                     "key_dl",
                                     "key_down",
                                     "key_eic",
                                     "key_eol",
                                     "key_eos",
                                     "key_f0",
                                     "key_f1",
                                     "key_f10",
                                     "key_f2",
                                     "key_f3",
                                     "key_f4",
                                     "key_f5",
                                     "key_f6",
                                     "key_f7",
                                     "key_f8",
                                     "key_f9",
                                     "key_home",
                                     "key_ic",
                                     "key_il",
                                     "key_left",
                                     "key_ll",
                                     "key_npage",
                                     "key_ppage",
                                     "key_right",
                                     "key_sf",
                                     "key_sr",
                                     "key_stab",
                                     "key_up",
                                     "keypad_local",
                                     "keypad_xmit",
                                     "lab_f0",
                                     "lab_f1",
                                     "lab_f10",
                                     "lab_f2",
                                     "lab_f3",
                                     "lab_f4",
                                     "lab_f5",
                                     "lab_f6",
                                     "lab_f7",
                                     "lab_f8",
                                     "lab_f9",
                                     "meta_off",
                                     "meta_on",
                                     "newline",
                                     "pad_char",
                                     "parm_dch",
                                     "parm_delete_line",
                                     "parm_down_cursor",
                                     "parm_ich",
                                     "parm_index",
                                     "parm_insert_line",
                                     "parm_left_cursor",
                                     "parm_right_cursor",
                                     "parm_rindex",
                                     "parm_up_cursor",
                                     "pkey_key",
                                     "pkey_local",
                                     "pkey_xmit",
                                     "print_screen",
                                     "prtr_off",
                                     "prtr_on",
                                     "repeat_char",
                                     "reset_1string",
                                     "reset_2string",
                                     "reset_3string",
                                     "reset_file",
                                     "restore_cursor",
                                     "row_address",
                                     "save_cursor",
                                     "scroll_forward",
                                     "scroll_reverse",
                                     "set_attributes",
                                     "set_tab",
                                     "set_window",
                                     "tab",
                                     "to_status_line",
                                     "underline_char",
                                     "up_half_line",
                                     "init_prog",
                                     "key_a1",
                                     "key_a3",
                                     "key_b2",
                                     "key_c1",
                                     "key_c3",
                                     "prtr_non",
                                     "char_padding",
                                     "acs_chars",
                                     "plab_norm",
                                     "key_btab",
                                     "enter_xon_mode",
                                     "exit_xon_mode",
                                     "enter_am_mode",
                                     "exit_am_mode",
                                     "xon_character",
                                     "xoff_character",
                                     "ena_acs",
                                     "label_on",
                                     "label_off",
                                     "key_beg",
                                     "key_cancel",
                                     "key_close",
                                     "key_command",
                                     "key_copy",
                                     "key_create",
                                     "key_end",
                                     "key_enter",
                                     "key_exit",
                                     "key_find",
                                     "key_help",
                                     "key_mark",
                                     "key_message",
                                     "key_move",
                                     "key_next",
                                     "key_open",
                                     "key_options",
                                     "key_previous",
                                     "key_print",
                                     "key_redo",
                                     "key_reference",
                                     "key_refresh",
                                     "key_replace",
                                     "key_restart",
                                     "key_resume",
                                     "key_save",
                                     "key_suspend",
                                     "key_undo",
                                     "key_sbeg",
                                     "key_scancel",
                                     "key_scommand",
                                     "key_scopy",
                                     "key_screate",
                                     "key_sdc",
                                     "key_sdl",
                                     "key_select",
                                     "key_send",
                                     "key_seol",
                                     "key_sexit",
                                     "key_sfind",
                                     "key_shelp",
                                     "key_shome",
                                     "key_sic",
                                     "key_sleft",
                                     "key_smessage",
                                     "key_smove",
                                     "key_snext",
                                     "key_soptions",
                                     "key_sprevious",
                                     "key_sprint",
                                     "key_sredo",
                                     "key_sreplace",
                                     "key_sright",
                                     "key_srsume",
                                     "key_ssave",
                                     "key_ssuspend",
                                     "key_sundo",
                                     "req_for_input",
                                     "key_f11",
                                     "key_f12",
                                     "key_f13",
                                     "key_f14",
                                     "key_f15",
                                     "key_f16",
                                     "key_f17",
                                     "key_f18",
                                     "key_f19",
                                     "key_f20",
                                     "key_f21",
                                     "key_f22",
                                     "key_f23",
                                     "key_f24",
                                     "key_f25",
                                     "key_f26",
                                     "key_f27",
                                     "key_f28",
                                     "key_f29",
                                     "key_f30",
                                     "key_f31",
                                     "key_f32",
                                     "key_f33",
                                     "key_f34",
                                     "key_f35",
                                     "key_f36",
                                     "key_f37",
                                     "key_f38",
                                     "key_f39",
                                     "key_f40",
                                     "key_f41",
                                     "key_f42",
                                     "key_f43",
                                     "key_f44",
                                     "key_f45",
                                     "key_f46",
                                     "key_f47",
                                     "key_f48",
                                     "key_f49",
                                     "key_f50",
                                     "key_f51",
                                     "key_f52",
                                     "key_f53",
                                     "key_f54",
                                     "key_f55",
                                     "key_f56",
                                     "key_f57",
                                     "key_f58",
                                     "key_f59",
                                     "key_f60",
                                     "key_f61",
                                     "key_f62",
                                     "key_f63",
                                     "clr_bol",
                                     "clear_margins",
                                     "set_left_margin",
                                     "set_right_margin",
                                     "label_format",
                                     "set_clock",
                                     "display_clock",
                                     "remove_clock",
                                     "create_window",
                                     "goto_window",
                                     "hangup",
                                     "dial_phone",
                                     "quick_dial",
                                     "tone",
                                     "pulse",
                                     "flash_hook",
                                     "fixed_pause",
                                     "wait_tone",
                                     "user0",
                                     "user1",
                                     "user2",
                                     "user3",
                                     "user4",
                                     "user5",
                                     "user6",
                                     "user7",
                                     "user8",
                                     "user9",
                                     "orig_pair",
                                     "orig_colors",
                                     "initialize_color",
                                     "initialize_pair",
                                     "set_color_pair",
                                     "set_foreground",
                                     "set_background",
                                     "change_char_pitch",
                                     "change_line_pitch",
                                     "change_res_horz",
                                     "change_res_vert",
                                     "define_char",
                                     "enter_doublewide_mode",
                                     "enter_draft_quality",
                                     "enter_italics_mode",
                                     "enter_leftward_mode",
                                     "enter_micro_mode",
                                     "enter_near_letter_quality",
                                     "enter_normal_quality",
                                     "enter_shadow_mode",
                                     "enter_subscript_mode",
                                     "enter_superscript_mode",
                                     "enter_upward_mode",
                                     "exit_doublewide_mode",
                                     "exit_italics_mode",
                                     "exit_leftward_mode",
                                     "exit_micro_mode",
                                     "exit_shadow_mode",
                                     "exit_subscript_mode",
                                     "exit_superscript_mode",
                                     "exit_upward_mode",
                                     "micro_column_address",
                                     "micro_down",
                                     "micro_left",
                                     "micro_right",
                                     "micro_row_address",
                                     "micro_up",
                                     "order_of_pins",
                                     "parm_down_micro",
                                     "parm_left_micro",
                                     "parm_right_micro",
                                     "parm_up_micro",
                                     "select_char_set",
                                     "set_bottom_margin",
                                     "set_bottom_margin_parm",
                                     "set_left_margin_parm",
                                     "set_right_margin_parm",
                                     "set_top_margin",
                                     "set_top_margin_parm",
                                     "start_bit_image",
                                     "start_char_set_def",
                                     "stop_bit_image",
                                     "stop_char_set_def",
                                     "subscript_characters",
                                     "superscript_characters",
                                     "these_cause_cr",
                                     "zero_motion",
                                     "char_set_names",
                                     "key_mouse",
                                     "mouse_info",
                                     "req_mouse_pos",
                                     "get_mouse",
                                     "set_a_foreground",
                                     "set_a_background",
                                     "pkey_plab",
                                     "device_type",
                                     "code_set_init",
                                     "set0_des_seq",
                                     "set1_des_seq",
                                     "set2_des_seq",
                                     "set3_des_seq",
                                     "set_lr_margin",
                                     "set_tb_margin",
                                     "bit_image_repeat",
                                     "bit_image_newline",
                                     "bit_image_carriage_return",
                                     "color_names",
                                     "define_bit_image_region",
                                     "end_bit_image_region",
                                     "set_color_band",
                                     "set_page_length",
                                     "display_pc_char",
                                     "enter_pc_charset_mode",
                                     "exit_pc_charset_mode",
                                     "enter_scancode_mode",
                                     "exit_scancode_mode",
                                     "pc_term_options",
                                     "scancode_escape",
                                     "alt_scancode_esc",
                                     "enter_horizontal_hl_mode",
                                     "enter_left_hl_mode",
                                     "enter_low_hl_mode",
                                     "enter_right_hl_mode",
                                     "enter_top_hl_mode",
                                     "enter_vertical_hl_mode",
                                     "set_a_attributes",
                                     "set_pglen_inch",
                                     "termcap_init2",
                                     "termcap_reset",
                                     "linefeed_if_not_lf",
                                     "backspace_if_not_bs",
                                     "other_non_function_keys",
                                     "arrow_key_map",
                                     "acs_ulcorner",
                                     "acs_llcorner",
                                     "acs_urcorner",
                                     "acs_lrcorner",
                                     "acs_ltee",
                                     "acs_rtee",
                                     "acs_btee",
                                     "acs_ttee",
                                     "acs_hline",
                                     "acs_vline",
                                     "acs_plus",
                                     "memory_lock",
                                     "memory_unlock",
                                     "box_chars_1"];

/// Short terminfo string capability names in ncurses table order.
pub static STRING_NAMES: &[&str] =
    &["cbt", "bel", "cr", "csr", "tbc", "clear", "el", "ed", "hpa", "cmdch", "cup", "cud1",
      "home", "civis", "cub1", "mrcup", "cnorm", "cuf1", "ll", "cuu1", "cvvis", "dch1", "dl1",
      "dsl", "hd", "smacs", "blink", "bold", "smcup", "smdc", "dim", "smir", "invis", "prot",
      "rev", "smso", "smul", "ech", "rmacs", "sgr0", "rmcup", "rmdc", "rmir", "rmso", "rmul",
      "flash", "ff", "fsl", "is1", "is2", "is3", "if", "ich1", "il1", "ip", "kbs", "ktbc", "kclr",
      "kctab", "kdch1", "kdl1", "kcud1", "krmir", "kel", "ked", "kf0", "kf1", "kf10", "kf2",
      "kf3", "kf4", "kf5", "kf6", "kf7", "kf8", "kf9", "khome", "kich1", "kil1", "kcub1", "kll",
      "knp", "kpp", "kcuf1", "kind", "kri", "khts", "kcuu1", "rmkx", "smkx", "lf0", "lf1", "lf10",
      "lf2", "lf3", "lf4", "lf5", "lf6", "lf7", "lf8", "lf9", "rmm", "smm", "nel", "pad", "dch",
      "dl", "cud", "ich", "indn", "il", "cub", "cuf", "rin", "cuu", "pfkey", "pfloc", "pfx",
      "mc0", "mc4", "mc5", "rep", "rs1", "rs2", "rs3", "rf", "rc", "vpa", "sc", "ind", "ri",
      "sgr", "hts", "wind", "ht", "tsl", "uc", "hu", "iprog", "ka1", "ka3", "kb2", "kc1", "kc3",
      "mc5p", "rmp", "acsc", "pln", "kcbt", "smxon", "rmxon", "smam", "rmam", "xonc", "xoffc",
      "enacs", "smln", "rmln", "kbeg", "kcan", "kclo", "kcmd", "kcpy", "kcrt", "kend", "kent",
      "kext", "kfnd", "khlp", "kmrk", "kmsg", "kmov", "knxt", "kopn", "kopt", "kprv", "kprt",
      "krdo", "kref", "krfr", "krpl", "krst", "kres", "ksav", "kspd", "kund", "kBEG", "kCAN",
      "kCMD", "kCPY", "kCRT", "kDC", "kDL", "kslt", "kEND", "kEOL", "kEXT", "kFND", "kHLP",
      "kHOM", "kIC", "kLFT", "kMSG", "kMOV", "kNXT", "kOPT", "kPRV", "kPRT", "kRDO", "kRPL",
      "kRIT", "kRES", "kSAV", "kSPD", "kUND", "rfi", "kf11", "kf12", "kf13", "kf14", "kf15",
      "kf16", "kf17", "kf18", "kf19", "kf20", "kf21", "kf22", "kf23", "kf24", "kf25", "kf26",
      "kf27", "kf28", "kf29", "kf30", "kf31", "kf32", "kf33", "kf34", "kf35", "kf36", "kf37",
      "kf38", "kf39", "kf40", "kf41", "kf42", "kf43", "kf44", "kf45", "kf46", "kf47", "kf48",
      "kf49", "kf50", "kf51", "kf52", "kf53", "kf54", "kf55", "kf56", "kf57", "kf58", "kf59",
      "kf60", "kf61", "kf62", "kf63", "el1", "mgc", "smgl", "smgr", "fln", "sclk", "dclk",
      "rmclk", "cwin", "wingo", "hup", "dial", "qdial", "tone", "pulse", "hook", "pause", "wait",
      "u0", "u1", "u2", "u3", "u4", "u5", "u6", "u7", "u8", "u9", "op", "oc", "initc", "initp",
      "scp", "setf", "setb", "cpi", "lpi", "chr", "cvr", "defc", "swidm", "sdrfq", "sitm", "slm",
      "smicm", "snlq", "snrmq", "sshm", "ssubm", "ssupm", "sum", "rwidm", "ritm", "rlm", "rmicm",
      "rshm", "rsubm", "rsupm", "rum", "mhpa", "mcud1", "mcub1", "mcuf1", "mvpa", "mcuu1",
      "porder", "mcud", "mcub", "mcuf", "mcuu", "scs", "smgb", "smgbp", "smglp", "smgrp", "smgt",
      "smgtp", "sbim", "scsd", "rbim", "rcsd", "subcs", "supcs", "docr", "zerom", "csnm", "kmous",
      "minfo", "reqmp", "getm", "setaf", "setab", "pfxl", "devt", "csin", "s0ds", "s1ds", "s2ds",
      "s3ds", "smglr", "smgtb", "birep", "binel", "bicr", "colornm", "defbi", "endbi", "setcolor",
      "slines", "dispc", "smpch", "rmpch", "smsc", "rmsc", "pctrm", "scesc", "scesa", "ehhlm",
      "elhlm", "elohlm", "erhlm", "ethlm", "evhlm", "sgr1", "slength", "OTi2", "OTrs", "OTnl",
      "OTbc", "OTko", "OTma", "OTG2", "OTG3", "OTG1", "OTG4", "OTGR", "OTGL", "OTGU", "OTGD",
      "OTGH", "OTGV", "OTGC", "meml", "memu", "box1"];

/// Two-character termcap boolean codes in ncurses table order.
pub static BOOL_CODES: &[&str] =
    &["bw", "am", "xb", "xs", "xn", "eo", "gn", "hc", "km", "hs", "in", "da", "db", "mi",
      "ms", "os", "es", "xt", "hz", "ul", "xo", "nx", "5i", "HC", "NR", "NP", "ND", "cc",
      "ut", "hl", "YA", "YB", "YC", "YD", "YE", "YF", "YG", "bs", "ns", "nc", "MT", "NL",
      "pt", "xr"];

/// Two-character termcap numeric codes in ncurses table order.
pub static NUMBER_CODES: &[&str] =
    &["co", "it", "li", "lm", "sg", "pb", "vt", "ws", "Nl", "lh", "lw", "ma", "MW", "Co",
      "pa", "NC", "Ya", "Yb", "Yc", "Yd", "Ye", "Yf", "Yg", "Yh", "Yi", "Yj", "Yk", "Yl",
      "Ym", "Yn", "BT", "Yo", "Yp", "ug", "dC", "dN", "dB", "dT", "kn"];

/// Two-character termcap string codes in ncurses table order.
pub static STRING_CODES: &[&str] =
    &["bt", "bl", "cr", "cs", "ct", "cl", "ce", "cd", "ch", "CC", "cm", "do", "ho", "vi",
      "le", "CM", "ve", "nd", "ll", "up", "vs", "dc", "dl", "ds", "hd", "as", "mb", "md",
      "ti", "dm", "mh", "im", "mk", "mp", "mr", "so", "us", "ec", "ae", "me", "te", "ed",
      "ei", "se", "ue", "vb", "ff", "fs", "i1", "is", "i3", "if", "ic", "al", "ip", "kb",
      "ka", "kC", "kt", "kD", "kL", "kd", "kM", "kE", "kS", "k0", "k1", "k;", "k2", "k3",
      "k4", "k5", "k6", "k7", "k8", "k9", "kh", "kI", "kA", "kl", "kH", "kN", "kP", "kr",
      "kF", "kR", "kT", "ku", "ke", "ks", "l0", "l1", "la", "l2", "l3", "l4", "l5", "l6",
      "l7", "l8", "l9", "mo", "mm", "nw", "pc", "DC", "DL", "DO", "IC", "SF", "AL", "LE",
      "RI", "SR", "UP", "pk", "pl", "px", "ps", "pf", "po", "rp", "r1", "r2", "r3", "rf",
      "rc", "cv", "sc", "sf", "sr", "sa", "st", "wi", "ta", "ts", "uc", "hu", "iP", "K1",
      "K3", "K2", "K4", "K5", "pO", "rP", "ac", "pn", "kB", "SX", "RX", "SA", "RA", "XN",
      "XF", "eA", "LO", "LF", "@1", "@2", "@3", "@4", "@5", "@6", "@7", "@8", "@9", "@0",
      "%1", "%2", "%3", "%4", "%5", "%6", "%7", "%8", "%9", "%0", "&1", "&2", "&3", "&4",
      "&5", "&6", "&7", "&8", "&9", "&0", "*1", "*2", "*3", "*4", "*5", "*6", "*7", "*8",
      "*9", "*0", "#1", "#2", "#3", "#4", "%a", "%b", "%c", "%d", "%e", "%f", "%g", "%h",
      "%i", "%j", "!1", "!2", "!3", "RF", "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8",
      "F9", "FA", "FB", "FC", "FD", "FE", "FF", "FG", "FH", "FI", "FJ", "FK", "FL", "FM",
      "FN", "FO", "FP", "FQ", "FR", "FS", "FT", "FU", "FV", "FW", "FX", "FY", "FZ", "Fa",
      "Fb", "Fc", "Fd", "Fe", "Ff", "Fg", "Fh", "Fi", "Fj", "Fk", "Fl", "Fm", "Fn", "Fo",
      "Fp", "Fq", "Fr", "cb", "MC", "ML", "MR", "Lf", "SC", "DK", "RC", "CW", "WG", "HU",
      "DI", "QD", "TO", "PU", "fh", "PA", "WA", "u0", "u1", "u2", "u3", "u4", "u5", "u6",
      "u7", "u8", "u9", "op", "oc", "Ic", "Ip", "sp", "Sf", "Sb", "ZA", "ZB", "ZC", "ZD",
      "ZE", "ZF", "ZG", "ZH", "ZI", "ZJ", "ZK", "ZL", "ZM", "ZN", "ZO", "ZP", "ZQ", "ZR",
      "ZS", "ZT", "ZU", "ZV", "ZW", "ZX", "ZY", "ZZ", "Za", "Zb", "Zc", "Zd", "Ze", "Zf",
      "Zg", "Zh", "Zi", "Zj", "Zk", "Zl", "Zm", "Zn", "Zo", "Zp", "Zq", "Zr", "Zs", "Zt",
      "Zu", "Zv", "Zw", "Zx", "Zy", "Km", "Mi", "RQ", "Gm", "AF", "AB", "xl", "dv", "ci",
      "s0", "s1", "s2", "s3", "ML", "MT", "Xy", "Zz", "Yv", "Yw", "Yx", "Yy", "Yz", "YZ",
      "S1", "S2", "S3", "S4", "S5", "S6", "S7", "S8", "Xh", "Xl", "Xo", "Xr", "Xt", "Xv",
      "sA", "YI", "i2", "rs", "nl", "bc", "ko", "ma", "G2", "G3", "G1", "G4", "GR", "GL",
      "GU", "GD", "GH", "GV", "GC", "ml", "mu", "bx"];

/// The terminfo capname a termcap two-letter code refers to.
///
/// This is the conversion libedit needs and `term` 1.2.1 could not do.
/// `settc`, `echotc` and `EL_SETTC` all take a name a user typed — at a
/// prompt, or in `.editrc` — and what users type is termcap: `cl`, not
/// `clear`. Everything else in this crate is keyed by capname, so this is the
/// one place that translation happens.
///
/// Both directions are linear over ~500 entries, and run once for each name
/// someone types, which is not a rate worth indexing for.
///
/// # Three codes are ambiguous
///
/// The termcap namespace collides with itself, and ncurses resolves it by
/// knowing which type it wants. This searches booleans, then numbers, then
/// strings, so:
///
/// ```text
/// MT   bool OTMT   wins over  str smgtb
/// ma   num  ma     wins over  str OTma
/// ML   str  smgl   wins over  str smglr
/// ```
///
/// Measured against `Caps` 6.5, and those three are the whole list. All of
/// them are obsolete or margin-setting capabilities that `settc` and `echotc`
/// are not plausibly asked for, so the order is a choice rather than a
/// problem — but it IS a choice, and a caller that knows the type should
/// index the `*codes` array it wants directly instead of asking here.
#[must_use]
pub fn capname_for_termcap(code: &str) -> Option<&'static str> {
    if code.is_empty() {
        return None;
    }
    for (codes, names) in [
        (BOOL_CODES, BOOL_NAMES),
        (NUMBER_CODES, NUMBER_NAMES),
        (STRING_CODES, STRING_NAMES),
    ] {
        if let Some(i) = codes.iter().position(|&c| c == code) {
            return Some(names[i]);
        }
    }
    None
}

/// The termcap two-letter code for a terminfo capname, where one exists.
///
/// The inverse of [`capname_for_termcap`], for reporting a capability back to
/// a user in the vocabulary they used.
#[must_use]
pub fn termcap_for_capname(name: &str) -> Option<&'static str> {
    for (names, codes) in [
        (BOOL_NAMES, BOOL_CODES),
        (NUMBER_NAMES, NUMBER_CODES),
        (STRING_NAMES, STRING_CODES),
    ] {
        if let Some(i) = names.iter().position(|&n| n == name) {
            return (!codes[i].is_empty()).then_some(codes[i]);
        }
    }
    None
}

#[cfg(test)]
mod test {
    use super::*;

    /// The three columns are one table in ncurses' `Caps` and must stay one
    /// here: an index into `STRING_NAMES` is an index into the compiled
    /// terminfo file's string table, so a length that drifts silently
    /// mislabels every capability after the drift.
    #[test]
    fn the_columns_are_the_same_length() {
        assert_eq!(BOOL_NAMES.len(), BOOL_LONG_NAMES.len());
        assert_eq!(BOOL_NAMES.len(), BOOL_CODES.len());
        assert_eq!(NUMBER_NAMES.len(), NUMBER_LONG_NAMES.len());
        assert_eq!(NUMBER_NAMES.len(), NUMBER_CODES.len());
        assert_eq!(STRING_NAMES.len(), STRING_LONG_NAMES.len());
        assert_eq!(STRING_NAMES.len(), STRING_CODES.len());
        // ncurses 6.5's own counts.
        assert_eq!((BOOL_NAMES.len(), NUMBER_NAMES.len(), STRING_NAMES.len()), (44, 39, 414));
    }

    /// The codes libedit's own `settc`/`echotc` documentation names, and the
    /// ones a `.editrc` is most likely to carry.
    #[test]
    fn the_codes_users_actually_type_resolve() {
        for (code, capname) in [
            ("cl", "clear"),
            ("ce", "el"),
            ("cd", "ed"),
            ("cm", "cup"),
            ("ho", "home"),
            ("nd", "cuf1"),
            ("up", "cuu1"),
            ("so", "smso"),
            ("se", "rmso"),
            ("us", "smul"),
            ("ue", "rmul"),
            ("md", "bold"),
            ("me", "sgr0"),
            ("bl", "bel"),
            ("co", "cols"),
            ("li", "lines"),
            ("am", "am"),
            ("pt", "OTpt"),
        ] {
            assert_eq!(
                capname_for_termcap(code),
                Some(capname),
                "termcap {code} should be terminfo {capname}"
            );
        }
    }

    #[test]
    fn the_mapping_round_trips() {
        for code in ["cl", "cm", "co", "li", "am", "ce"] {
            let name = capname_for_termcap(code).unwrap();
            assert_eq!(termcap_for_capname(name), Some(code));
        }
    }

    /// An unknown name is `None`, and the empty string is not a wildcard.
    ///
    /// Every capability turns out to HAVE a termcap code — 0 of 497 `Caps`
    /// rows carry `-`, measured — so the empty case is input validation
    /// rather than the common case I first assumed it was. `setaf` is `AF`,
    /// not absent.
    #[test]
    fn an_unknown_name_is_not_a_wildcard() {
        assert_eq!(capname_for_termcap(""), None);
        assert_eq!(capname_for_termcap("no-such-code"), None);
        assert_eq!(termcap_for_capname(""), None);
        assert_eq!(termcap_for_capname("no_such_capability"), None);
        assert_eq!(termcap_for_capname("setaf"), Some("AF"));
        assert!(
            [BOOL_CODES, NUMBER_CODES, STRING_CODES]
                .iter()
                .all(|t| t.iter().all(|c| !c.is_empty())),
            "a generated code is empty, so the empty-string guard now matters"
        );
    }

    /// Three termcap codes name two capabilities each, so which one wins is a
    /// choice this pins rather than an accident. Measured against `Caps` 6.5;
    /// these three are the whole list.
    #[test]
    fn the_ambiguous_codes_resolve_the_way_the_search_order_says() {
        assert_eq!(capname_for_termcap("MT"), Some("OTMT")); // over smgtb
        assert_eq!(capname_for_termcap("ma"), Some("ma")); // over OTma
        assert_eq!(capname_for_termcap("ML"), Some("smgl")); // over smglr

        let mut seen = std::collections::HashMap::new();
        let mut clashes = Vec::new();
        for table in [BOOL_CODES, NUMBER_CODES, STRING_CODES] {
            for &code in table {
                if seen.insert(code, ()).is_some() {
                    clashes.push(code);
                }
            }
        }
        clashes.sort_unstable();
        assert_eq!(clashes, ["ML", "MT", "ma"]);
    }

    /// Two capabilities `term` 1.2.1 misspelled by one character each, so
    /// neither could ever be found by name. Both are checked against
    /// ncurses' `Caps`, which is where the generated columns come from.
    #[test]
    fn the_two_misspelled_capnames_are_correct() {
        // Caps: magic_cookie_glitch_ul  OTug  num  ug  — was `UTug`.
        assert!(NUMBER_NAMES.contains(&"OTug"));
        assert!(!NUMBER_NAMES.contains(&"UTug"));
        assert_eq!(capname_for_termcap("ug"), Some("OTug"));

        // Caps: backspace_if_not_bs  OTbc  str  bc  — was `OTbs`, which is a
        // real capname, but a BOOLEAN one, so the string table carried a
        // duplicate of a different capability's name.
        assert_eq!(STRING_NAMES[397], "OTbc");
        assert_eq!(capname_for_termcap("bc"), Some("OTbc"));
        assert!(BOOL_NAMES.contains(&"OTbs"));
    }
}
