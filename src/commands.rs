//! Command IDs used by ribbon buttons / menus / shortcuts.

// File
pub const IDM_NEW: u32 = 40001;
pub const IDM_OPEN: u32 = 40002;
pub const IDM_SAVE: u32 = 40003;
pub const IDM_SAVE_AS: u32 = 40004;
pub const IDM_BACK: u32 = 40005; // leave backstage
pub const IDM_FILE: u32 = 40006; // open backstage
pub const IDM_EXIT: u32 = 40007;

// Edit
pub const IDM_UNDO: u32 = 40010;
pub const IDM_REDO: u32 = 40011;
pub const IDM_CUT: u32 = 40012;
pub const IDM_COPY: u32 = 40013;
pub const IDM_PASTE: u32 = 40014;
pub const IDM_SELECT_ALL: u32 = 40015;

// Font group
pub const IDM_FONT_GROW: u32 = 40020;
pub const IDM_FONT_SHRINK: u32 = 40021;
pub const IDM_CLEAR_FORMAT: u32 = 40022;
pub const IDM_BOLD: u32 = 40023;
pub const IDM_ITALIC: u32 = 40024;
pub const IDM_UNDERLINE: u32 = 40025;
pub const IDM_STRIKE: u32 = 40026;
pub const IDM_SUBSCRIPT: u32 = 40027;
pub const IDM_SUPERSCRIPT: u32 = 40028;
pub const IDM_TEXT_HIGHLIGHT: u32 = 40029;
pub const IDM_FONT_COLOR: u32 = 40030;
pub const IDM_FONT_DIALOG: u32 = 40031;
pub const IDM_CHAR_SPACING: u32 = 40032; // character spacing dialog/dropdown
pub const IDM_CHANGE_CASE: u32 = 40033;
pub const IDM_TEXT_EFFECTS: u32 = 40034;

// Paragraph group
pub const IDM_BULLETS: u32 = 40040;
pub const IDM_NUMBERING: u32 = 40041;
pub const IDM_MULTILEVEL: u32 = 40042;
pub const IDM_INDENT_DEC: u32 = 40043;
pub const IDM_INDENT_INC: u32 = 40044;
pub const IDM_SORT: u32 = 40045;
pub const IDM_SHOW_MARKS: u32 = 40046; // pilcrow
pub const IDM_ALIGN_LEFT: u32 = 40047;
pub const IDM_ALIGN_CENTER: u32 = 40048;
pub const IDM_ALIGN_RIGHT: u32 = 40049;
pub const IDM_ALIGN_JUSTIFY: u32 = 40050;
pub const IDM_LINE_SPACING: u32 = 40051; // dropdown
pub const IDM_SHADING: u32 = 40052;
pub const IDM_BORDERS: u32 = 40053;
pub const IDM_PARA_DIALOG: u32 = 40054; // space before/after etc.
pub const IDM_WORD_SPACING: u32 = 40055; // word spacing dialog

// Insert group
pub const IDM_PAGE_BREAK: u32 = 40060;
pub const IDM_SHAPES: u32 = 40061; // gallery dropdown
pub const IDM_TEXT_BOX: u32 = 40062;
pub const IDM_PICTURE: u32 = 40063;
pub const IDM_TABLE: u32 = 40064;
pub const IDM_SYMBOL: u32 = 40065;
pub const IDM_HYPERLINK: u32 = 40066;
pub const IDM_DATE_TIME: u32 = 40067;

// Layout
pub const IDM_MARGINS: u32 = 40070;
pub const IDM_ORIENTATION: u32 = 40071;
pub const IDM_PAGE_SIZE: u32 = 40072;
pub const IDM_COLUMNS: u32 = 40073;

// Shape format
pub const IDM_SHAPE_FILL: u32 = 40080;
pub const IDM_SHAPE_OUTLINE: u32 = 40081;
pub const IDM_SHAPE_MARGIN_DLG: u32 = 40082;
pub const IDM_SHAPE_DELETE: u32 = 40083;
pub const IDM_SHAPE_BRING_FWD: u32 = 40084;
pub const IDM_SHAPE_SEND_BACK: u32 = 40085;

// Zoom / view
pub const IDM_ZOOM_IN: u32 = 40090;
pub const IDM_ZOOM_OUT: u32 = 40091;

// App-internal
pub const IDM_ZOOM_SLIDER: u32 = 40100;

// Ribbon-internal control IDs
pub const IDC_FONT_NAME: u32 = 40200;
pub const IDC_FONT_SIZE: u32 = 40201;
pub const IDC_SHAPE_TEXT: u32 = 40202;

// Custom window messages
pub const WM_APP_SHAPE_SELECTED: u32 = 0x8000 + 1;
pub const WM_APP_SHAPE_DELETED: u32 = 0x8000 + 2;
pub const WM_APP_EXIT_SHAPES: u32 = 0x8000 + 3;
