// ═══════════════════════════════════════════════════════════════════════════════
//  STORY — Types: enums, constants, ProductStoryMeta
// ═══════════════════════════════════════════════════════════════════════════════

// ── ProductStoryMeta ─────────────────────────────────────────────────────────────
#[derive(Clone, Debug)]
pub struct ProductStoryMeta {
    pub event_id: String,
    pub event_slug: String,
    pub event_title: String,
}

// ── Alat (tool selection enum) ─────────────────────────────────────────────────
#[derive(Clone, PartialEq, Debug, Copy)]
pub(super) enum Alat {
    None,
    Teks,
    Stiker,
    Musik,
    Filter,
    Latar,
}

// ── Background colors & gradients ──────────────────────────────────────────────
pub(super) const BG_SOLID_COLORS: &[&str] = &[
    "#000000", "#1a1a2e", "#16213e", "#0f3460", "#533483", "#e94560", "#ff9f43", "#10ac84",
    "#0984e3", "#6c5ce7", "#fd79a8", "#00cec9", "#2d3436", "#636e72",
];

pub(super) const BG_GRADIENTS: &[(&str, &str, &str, &str)] = &[
    ("purple-haze", "linear-gradient(135deg,#667eea 0%,#764ba2 100%)", "#667eea", "#764ba2"),
    ("sunset",      "linear-gradient(135deg,#f093fb 0%,#f5576c 100%)", "#f093fb", "#f5576c"),
    ("ocean",       "linear-gradient(135deg,#4facfe 0%,#00f2fe 100%)", "#4facfe", "#00f2fe"),
    ("forest",      "linear-gradient(135deg,#43e97b 0%,#38f9d7 100%)", "#43e97b", "#38f9d7"),
    ("midnight",    "linear-gradient(135deg,#0f2027 0%,#203a43 50%,#2c5364 100%)", "#0f2027", "#2c5364"),
    ("candy",       "linear-gradient(135deg,#ff9a9e 0%,#fecfef 100%)", "#ff9a9e", "#fecfef"),
];

pub(super) const STIKER: &[&str] = &[
    "❤️","🔥","😂","😍","👏","🎉","💯","✨","🙌","😭","🥰","🤔","👍","🎵","🎸","🌟",
    "🎤","🏆","🎶","💫","🌈","🍀","🎯","💎","🦋","🌺","🔮","🍕","🌙","☀️",
];

pub(super) const DAFTAR_MUSIK: &[(&str, &str, &str)] = &[
    ("1","As It Was","Harry Styles"),
    ("2","Heat Waves","Glass Animals"),
    ("3","Stay","The Kid LAROI & Justin Bieber"),
    ("4","Levitating","Dua Lipa"),
    ("5","Good 4 U","Olivia Rodrigo"),
    ("6","Montero","Lil Nas X"),
    ("7","Peaches","Justin Bieber"),
    ("8","Kiss Me More","Doja Cat"),
    ("9","Save Your Tears","The Weeknd"),
    ("10","Butter","BTS"),
];

pub(super) const DAFTAR_FILTER: &[(&str, &str)] = &[
    ("normal","Normal"),("clarendon","Cerah"),("gingham","Hangat"),("moon","Monokrom"),
    ("lark","Lark"),("reyes","Reyes"),("juno","Juno"),("slumber","Slumber"),
    ("crema","Crema"),("ludwig","Ludwig"),
];

pub(super) const WARNA_TEKS: &[&str] = &[
    "#ffffff","#000000","#ff3040","#ffcc00","#39ff8a","#4f6bff","#ff00ff","#00ffff",
    "#ff6b35","#7209b7","#f72585","#4cc9f0",
];
