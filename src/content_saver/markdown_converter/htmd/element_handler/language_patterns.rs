//! Language pattern definitions for weighted scoring detection
//!
//! Each language has patterns categorized by specificity:
//! - Unique (10pts): Only this language
//! - Strong (8pts): Very indicative
//! - Medium (5pts): Shared but suggestive
//! - Weak (2pts): Mildly suggestive
//! - Negative (-10pts): Disqualifies

use super::language_inference::{LanguageDefinition, PatternCategory::*, WeightedPattern};

pub static RUST: LanguageDefinition = LanguageDefinition {
    name: "rust",
    patterns: &[
        // Unique to Rust (10 pts each)
        WeightedPattern::new(r#"fn\s+\w+\s*\([^)]*\)\s*->"#, Unique),
        WeightedPattern::new(r#"impl\s+\w+\s+for\s+"#, Unique),
        WeightedPattern::new(r#"impl\s+\w+\s*\{"#, Unique),
        WeightedPattern::new(r##"#\[derive\("##, Unique),
        WeightedPattern::new(r##"#\[\w+\]"##, Unique),
        WeightedPattern::new(r#"pub\s*\(crate\)"#, Unique),
        WeightedPattern::new(r#"&'[a-z]+\s"#, Unique),
        WeightedPattern::new(r#"Box<dyn\s"#, Unique),
        WeightedPattern::new(r#"Arc<Mutex<"#, Unique),
        WeightedPattern::new(r#"Arc<RwLock<"#, Unique),
        WeightedPattern::new(r#"Rc<RefCell<"#, Unique),
        WeightedPattern::new(r#"Result<[^>]+,\s*[^>]+>"#, Unique),
        WeightedPattern::new(r#"Option<[^>]+>"#, Unique),
        WeightedPattern::new(r#"vec!\["#, Unique),
        WeightedPattern::new(r#"println!\("#, Unique),
        WeightedPattern::new(r#"eprintln!\("#, Unique),
        WeightedPattern::new(r#"format!\("#, Unique),
        WeightedPattern::new(r#"panic!\("#, Unique),
        WeightedPattern::new(r#"assert!\("#, Unique),
        WeightedPattern::new(r#"assert_eq!\("#, Unique),
        WeightedPattern::new(r#"\.unwrap\(\)"#, Unique),
        WeightedPattern::new(r#"\.expect\("#, Unique),
        WeightedPattern::new(r#"\.unwrap_or\("#, Unique),
        WeightedPattern::new(r#"\.unwrap_or_else\("#, Unique),
        WeightedPattern::new(r#"async\s+fn\s"#, Unique),
        WeightedPattern::new(r#"\.await\b"#, Unique),
        WeightedPattern::new(r#"match\s+\w+\s*\{"#, Unique),
        WeightedPattern::new(r#"\s=>\s"#, Unique),
        WeightedPattern::new(r#"::new\(\)"#, Unique),
        WeightedPattern::new(r#"&mut\s"#, Unique),
        WeightedPattern::new(r#"&self\b"#, Unique),
        WeightedPattern::new(r#"\bSelf\b"#, Unique),
        WeightedPattern::new(r#"\btrait\s+\w+"#, Unique),
        WeightedPattern::new(r#"struct\s+\w+\s*[<{]"#, Unique),
        WeightedPattern::new(r#"enum\s+\w+\s*\{"#, Unique),
        WeightedPattern::new(r#"\bmod\s+\w+"#, Unique),
        WeightedPattern::new(r#"use\s+\w+::"#, Unique),
        WeightedPattern::new(r#"use\s+super::"#, Unique),
        WeightedPattern::new(r#"use\s+crate::"#, Unique),
        WeightedPattern::new(r#"pub\s+use\s"#, Unique),
        WeightedPattern::new(r#"if\s+let\s+Some"#, Unique),
        WeightedPattern::new(r#"if\s+let\s+Ok"#, Unique),
        WeightedPattern::new(r#"while\s+let\s"#, Unique),
        WeightedPattern::new(r#"\.iter\(\)"#, Unique),
        WeightedPattern::new(r#"\.into_iter\(\)"#, Unique),
        WeightedPattern::new(r#"\.collect::<"#, Unique),
        WeightedPattern::new(r#"\.map\(\|"#, Unique),
        WeightedPattern::new(r#"\.filter\(\|"#, Unique),
        WeightedPattern::new(r#"\.and_then\(\|"#, Unique),

        // Strong indicators (8 pts each)
        WeightedPattern::new(r#"pub\s+fn\s"#, Strong),
        WeightedPattern::new(r#"pub\s+struct\s"#, Strong),
        WeightedPattern::new(r#"pub\s+enum\s"#, Strong),
        WeightedPattern::new(r#"pub\s+type\s"#, Strong),
        WeightedPattern::new(r#"pub\s+const\s"#, Strong),
        WeightedPattern::new(r#"pub\s+static\s"#, Strong),
        WeightedPattern::new(r#"let\s+mut\s"#, Strong),
        WeightedPattern::new(r#"\blet\s+\w+\s*:"#, Strong),
        WeightedPattern::new(r#"->\s*Result"#, Strong),
        WeightedPattern::new(r#"->\s*Option"#, Strong),
        WeightedPattern::new(r#"->\s*Self"#, Strong),
        WeightedPattern::new(r#"\.clone\(\)"#, Strong),
        WeightedPattern::new(r#"\.to_string\(\)"#, Strong),
        WeightedPattern::new(r#"\.as_str\(\)"#, Strong),
        WeightedPattern::new(r#"\.as_ref\(\)"#, Strong),
        WeightedPattern::new(r#"crate::"#, Strong),
        WeightedPattern::new(r#"super::"#, Strong),

        // Medium indicators (5 pts each)
        WeightedPattern::new(r#"\bfn\s+\w+"#, Medium),
        WeightedPattern::new(r#"\bconst\s+\w+"#, Medium),
        WeightedPattern::new(r#"\bstatic\s+\w+"#, Medium),
        WeightedPattern::new(r#"\btype\s+\w+"#, Medium),
        WeightedPattern::new(r#"\bwhere\s"#, Medium),
        WeightedPattern::new(r#"\bdyn\s"#, Medium),
        WeightedPattern::new(r#"Box<"#, Medium),
        WeightedPattern::new(r#"Vec<"#, Medium),
        WeightedPattern::new(r#"HashMap<"#, Medium),
        WeightedPattern::new(r#"HashSet<"#, Medium),
        WeightedPattern::new(r#"BTreeMap<"#, Medium),
        WeightedPattern::new(r#"\bString\b"#, Medium),
        WeightedPattern::new(r#"\bi8\b"#, Medium),
        WeightedPattern::new(r#"\bi16\b"#, Medium),
        WeightedPattern::new(r#"\bi32\b"#, Medium),
        WeightedPattern::new(r#"\bi64\b"#, Medium),
        WeightedPattern::new(r#"\bu8\b"#, Medium),
        WeightedPattern::new(r#"\bu16\b"#, Medium),
        WeightedPattern::new(r#"\bu32\b"#, Medium),
        WeightedPattern::new(r#"\bu64\b"#, Medium),
        WeightedPattern::new(r#"\busize\b"#, Medium),
        WeightedPattern::new(r#"\bisize\b"#, Medium),
        WeightedPattern::new(r#"\bf32\b"#, Medium),
        WeightedPattern::new(r#"\bf64\b"#, Medium),
        WeightedPattern::new(r#"\bbool\b"#, Medium),

        // Negative patterns
        WeightedPattern::new(r#"\bfunction\s+\w+"#, Negative),  // JS
        WeightedPattern::new(r#"\bdef\s+\w+\s*\("#, Negative),  // Python
        WeightedPattern::new(r#"\bclass\s+\w+\s*:"#, Negative), // Python
        WeightedPattern::new(r#"public\s+class\s"#, Negative),  // Java
        WeightedPattern::new(r#"\bfunc\s+\w+"#, Negative),      // Go/Swift
    ],
};

pub static PYTHON: LanguageDefinition = LanguageDefinition {
    name: "python",
    patterns: &[
        // Unique to Python (10 pts each)
        WeightedPattern::new(r#"def\s+\w+\s*\([^)]*\)\s*:"#, Unique),
        WeightedPattern::new(r#"class\s+\w+\s*(\([^)]*\))?\s*:"#, Unique),
        WeightedPattern::new(r#"if\s+__name__\s*==\s*['"]__main__['"]"#, Unique),
        WeightedPattern::new(r#"from\s+\w+\s+import\s"#, Unique),
        WeightedPattern::new(r#"import\s+\w+\s+as\s+\w+"#, Unique),
        WeightedPattern::new(r#"@\w+\s*(\([^)]*\))?\s*\n\s*def\s"#, Unique),
        WeightedPattern::new(r#"@property\b"#, Unique),
        WeightedPattern::new(r#"@staticmethod\b"#, Unique),
        WeightedPattern::new(r#"@classmethod\b"#, Unique),
        WeightedPattern::new(r#"@dataclass\b"#, Unique),
        WeightedPattern::new(r#"self\.\w+"#, Unique),
        WeightedPattern::new(r#"self\s*,"#, Unique),
        WeightedPattern::new(r#"\(self\)"#, Unique),
        WeightedPattern::new(r#"def\s+__init__\s*\("#, Unique),
        WeightedPattern::new(r#"def\s+__str__\s*\("#, Unique),
        WeightedPattern::new(r#"def\s+__repr__\s*\("#, Unique),
        WeightedPattern::new(r#"def\s+__len__\s*\("#, Unique),
        WeightedPattern::new(r#"def\s+__getitem__\s*\("#, Unique),
        WeightedPattern::new(r#"def\s+__setitem__\s*\("#, Unique),
        WeightedPattern::new(r#"def\s+__iter__\s*\("#, Unique),
        WeightedPattern::new(r#"def\s+__next__\s*\("#, Unique),
        WeightedPattern::new(r#"def\s+__enter__\s*\("#, Unique),
        WeightedPattern::new(r#"def\s+__exit__\s*\("#, Unique),
        WeightedPattern::new(r#"raise\s+\w+Error"#, Unique),
        WeightedPattern::new(r#"except\s+\w+\s+as\s+\w+:"#, Unique),
        WeightedPattern::new(r#"except\s+\w+:"#, Unique),
        WeightedPattern::new(r#"with\s+\w+\([^)]*\)\s+as\s+\w+:"#, Unique),
        WeightedPattern::new(r#"async\s+def\s"#, Unique),
        WeightedPattern::new(r#"await\s+\w+"#, Unique),
        WeightedPattern::new(r#"yield\s+from\s"#, Unique),
        WeightedPattern::new(r#"lambda\s+\w*:"#, Unique),
        WeightedPattern::new(r#"\[\s*\w+\s+for\s+\w+\s+in\s"#, Unique),
        WeightedPattern::new(r#"\{\s*\w+:\s*\w+\s+for\s+\w+\s+in\s"#, Unique),
        WeightedPattern::new(r#"f"[^"]*\{[^}]+\}[^"]*""#, Unique),
        WeightedPattern::new(r#"f'[^']*\{[^}]+\}[^']*'"#, Unique),
        WeightedPattern::new(r#"\bTrue\b"#, Unique),
        WeightedPattern::new(r#"\bFalse\b"#, Unique),
        WeightedPattern::new(r#"\bNone\b"#, Unique),
        WeightedPattern::new(r#"\band\b"#, Unique),
        WeightedPattern::new(r#"\bor\b"#, Unique),
        WeightedPattern::new(r#"\bnot\b"#, Unique),
        WeightedPattern::new(r#"\bis\s+not\b"#, Unique),
        WeightedPattern::new(r#"\bnot\s+in\b"#, Unique),
        WeightedPattern::new(r#"print\s*\("#, Unique),
        WeightedPattern::new(r#"len\s*\("#, Unique),
        WeightedPattern::new(r#"range\s*\("#, Unique),
        WeightedPattern::new(r#"enumerate\s*\("#, Unique),
        WeightedPattern::new(r#"zip\s*\("#, Unique),
        WeightedPattern::new(r#"isinstance\s*\("#, Unique),
        WeightedPattern::new(r#"hasattr\s*\("#, Unique),
        WeightedPattern::new(r#"getattr\s*\("#, Unique),
        WeightedPattern::new(r#"setattr\s*\("#, Unique),
        WeightedPattern::new(r#"__all__\s*="#, Unique),
        WeightedPattern::new(r#"__version__\s*="#, Unique),

        // Strong indicators (8 pts each)
        WeightedPattern::new(r#"\bdef\s+\w+"#, Strong),
        WeightedPattern::new(r#"\bclass\s+\w+"#, Strong),
        WeightedPattern::new(r#"\bimport\s+\w+"#, Strong),
        WeightedPattern::new(r#"\bfrom\s+\w+"#, Strong),
        WeightedPattern::new(r#"if\s+\w+:"#, Strong),
        WeightedPattern::new(r#"elif\s+\w+:"#, Strong),
        WeightedPattern::new(r#"else\s*:"#, Strong),
        WeightedPattern::new(r#"for\s+\w+\s+in\s"#, Strong),
        WeightedPattern::new(r#"while\s+\w+:"#, Strong),
        WeightedPattern::new(r#"try\s*:"#, Strong),
        WeightedPattern::new(r#"except\s*:"#, Strong),
        WeightedPattern::new(r#"finally\s*:"#, Strong),
        WeightedPattern::new(r#"return\s"#, Strong),
        WeightedPattern::new(r#"yield\s"#, Strong),

        // Medium indicators (5 pts each)
        WeightedPattern::new(r##"#[^\n]+$"##, Medium),
        WeightedPattern::new(r#"'''[\s\S]*?'''"#, Medium),
        WeightedPattern::new(r#"\"\"\"[\s\S]*?\"\"\""#, Medium),
        WeightedPattern::new(r#":\s*int\b"#, Medium),
        WeightedPattern::new(r#":\s*str\b"#, Medium),
        WeightedPattern::new(r#":\s*float\b"#, Medium),
        WeightedPattern::new(r#":\s*bool\b"#, Medium),
        WeightedPattern::new(r#":\s*list\b"#, Medium),
        WeightedPattern::new(r#":\s*dict\b"#, Medium),
        WeightedPattern::new(r#"->\s*\w+"#, Medium),
        WeightedPattern::new(r#"\bpass\b"#, Medium),
        WeightedPattern::new(r#"\bbreak\b"#, Medium),
        WeightedPattern::new(r#"\bcontinue\b"#, Medium),
        WeightedPattern::new(r#"\bglobal\s+\w+"#, Medium),
        WeightedPattern::new(r#"\bnonlocal\s+\w+"#, Medium),
        WeightedPattern::new(r#"\bassert\s"#, Medium),

        // Negative patterns
        WeightedPattern::new(r#"\bfn\s+\w+"#, Negative),       // Rust
        WeightedPattern::new(r#"\bfunc\s+\w+"#, Negative),     // Go
        WeightedPattern::new(r#"function\s+\w+"#, Negative),   // JS
        WeightedPattern::new(r#"public\s+class\s"#, Negative), // Java
        WeightedPattern::new(r#"=>"#, Negative),               // Rust/JS
    ],
};

pub static JAVASCRIPT: LanguageDefinition = LanguageDefinition {
    name: "javascript",
    patterns: &[
        // Unique to JavaScript (10 pts each)
        WeightedPattern::new(r#"==="#, Unique),
        WeightedPattern::new(r#"!=="#, Unique),
        WeightedPattern::new(r#"=>\s*\{"#, Unique),
        WeightedPattern::new(r#"=>\s*[^{]"#, Unique),
        WeightedPattern::new(r#"export\s+default"#, Unique),
        WeightedPattern::new(r#"export\s+const"#, Unique),
        WeightedPattern::new(r#"export\s+function"#, Unique),
        WeightedPattern::new(r#"export\s+class"#, Unique),
        WeightedPattern::new(r#"import\s+.+\s+from\s+['""]"#, Unique),
        WeightedPattern::new(r#"import\s*\{[^}]+\}\s*from"#, Unique),
        WeightedPattern::new(r#"require\s*\(['""]"#, Unique),
        WeightedPattern::new(r#"module\.exports"#, Unique),
        WeightedPattern::new(r#"exports\.\w+"#, Unique),
        WeightedPattern::new(r#"async\s+function"#, Unique),
        WeightedPattern::new(r#"\.then\s*\("#, Unique),
        WeightedPattern::new(r#"\.catch\s*\("#, Unique),
        WeightedPattern::new(r#"\.finally\s*\("#, Unique),
        WeightedPattern::new(r#"new\s+Promise"#, Unique),
        WeightedPattern::new(r#"Promise\.(all|race|resolve|reject)"#, Unique),
        WeightedPattern::new(r#"class\s+\w+\s+extends"#, Unique),
        WeightedPattern::new(r#"constructor\s*\("#, Unique),
        WeightedPattern::new(r#"this\.\w+"#, Unique),
        WeightedPattern::new(r#"\btypeof\s"#, Unique),
        WeightedPattern::new(r#"\binstanceof\s"#, Unique),
        WeightedPattern::new(r#"\bundefined\b"#, Unique),
        WeightedPattern::new(r#"JSON\.(parse|stringify)"#, Unique),
        WeightedPattern::new(r#"console\.(log|error|warn|info|debug)"#, Unique),
        WeightedPattern::new(r#"document\.\w+"#, Unique),
        WeightedPattern::new(r#"window\.\w+"#, Unique),
        WeightedPattern::new(r#"addEventListener\s*\("#, Unique),
        WeightedPattern::new(r#"querySelector(All)?\s*\("#, Unique),
        WeightedPattern::new(r#"getElementById\s*\("#, Unique),
        WeightedPattern::new(r#"fetch\s*\("#, Unique),
        WeightedPattern::new(r#"\.map\s*\(\s*\w+\s*=>"#, Unique),
        WeightedPattern::new(r#"\.filter\s*\(\s*\w+\s*=>"#, Unique),
        WeightedPattern::new(r#"\.reduce\s*\(\s*\("#, Unique),
        WeightedPattern::new(r#"\.forEach\s*\("#, Unique),
        WeightedPattern::new(r#"\.find\s*\(\s*\w+\s*=>"#, Unique),
        WeightedPattern::new(r#"\.some\s*\(\s*\w+\s*=>"#, Unique),
        WeightedPattern::new(r#"\.every\s*\(\s*\w+\s*=>"#, Unique),
        WeightedPattern::new(r#"\.\.\.\w+"#, Unique),
        WeightedPattern::new(r#"`[^`]*\$\{"#, Unique),
        
        // Strong indicators (8 pts each)
        WeightedPattern::new(r#"\bconst\s+\w+"#, Strong),
        WeightedPattern::new(r#"\blet\s+\w+"#, Strong),
        WeightedPattern::new(r#"\bvar\s+\w+"#, Strong),
        WeightedPattern::new(r#"\bfunction\s+\w+"#, Strong),
        WeightedPattern::new(r#"\breturn\s"#, Strong),
        WeightedPattern::new(r#"if\s*\([^)]+\)"#, Strong),
        WeightedPattern::new(r#"else\s*\{"#, Strong),
        WeightedPattern::new(r#"for\s*\([^)]+\)"#, Strong),
        WeightedPattern::new(r#"while\s*\([^)]+\)"#, Strong),
        WeightedPattern::new(r#"\bclass\s+\w+"#, Strong),
        WeightedPattern::new(r#"\bnew\s+\w+"#, Strong),
        WeightedPattern::new(r#"\bthrow\s"#, Strong),
        WeightedPattern::new(r#"try\s*\{"#, Strong),
        WeightedPattern::new(r#"catch\s*\("#, Strong),
        
        // Medium indicators (5 pts each)
        WeightedPattern::new(r#"//[^\n]+$"#, Medium),
        WeightedPattern::new(r#"/\*[\s\S]*?\*/"#, Medium),
        WeightedPattern::new(r#"\btrue\b"#, Medium),
        WeightedPattern::new(r#"\bfalse\b"#, Medium),
        WeightedPattern::new(r#"\bnull\b"#, Medium),
        WeightedPattern::new(r#"Array\.(isArray|from|of)"#, Medium),
        WeightedPattern::new(r#"Object\.(keys|values|entries|assign)"#, Medium),
        
        // Negative patterns
        WeightedPattern::new(r#"\bfn\s+\w+"#, Negative),
        WeightedPattern::new(r#"\bdef\s+\w+\s*\("#, Negative),
        WeightedPattern::new(r#"\bfunc\s+\w+"#, Negative),
        WeightedPattern::new(r#"public\s+class\s"#, Negative),
    ],
};

pub static TYPESCRIPT: LanguageDefinition = LanguageDefinition {
    name: "typescript",
    patterns: &[
        // TypeScript-specific (10 pts each)
        WeightedPattern::new(r#":\s*(string|number|boolean|any|void|never|unknown)\b"#, Unique),
        WeightedPattern::new(r#"<[A-Z]\w*(\s*,\s*[A-Z]\w*)*>"#, Unique),
        WeightedPattern::new(r#"\binterface\s+\w+\s*(\{|extends)"#, Unique),
        WeightedPattern::new(r#"\btype\s+\w+\s*="#, Unique),
        WeightedPattern::new(r#"\benum\s+\w+\s*\{"#, Unique),
        WeightedPattern::new(r#"\bas\s+(string|number|boolean|any|const)"#, Unique),
        WeightedPattern::new(r#"<[A-Z]\w*>"#, Unique),
        WeightedPattern::new(r#"\bprivate\s+\w+:"#, Unique),
        WeightedPattern::new(r#"\bpublic\s+\w+:"#, Unique),
        WeightedPattern::new(r#"\bprotected\s+\w+:"#, Unique),
        WeightedPattern::new(r#"\breadonly\s+\w+"#, Unique),
        WeightedPattern::new(r#"\bkeyof\s"#, Unique),
        WeightedPattern::new(r#"\btypeof\s+\w+\s*\["#, Unique),
        WeightedPattern::new(r#"\bextends\s+\w+\s*\?"#, Unique),
        WeightedPattern::new(r#"Partial<"#, Unique),
        WeightedPattern::new(r#"Required<"#, Unique),
        WeightedPattern::new(r#"Readonly<"#, Unique),
        WeightedPattern::new(r#"Record<"#, Unique),
        WeightedPattern::new(r#"Pick<"#, Unique),
        WeightedPattern::new(r#"Omit<"#, Unique),
        WeightedPattern::new(r#"Exclude<"#, Unique),
        WeightedPattern::new(r#"Extract<"#, Unique),
        WeightedPattern::new(r#"NonNullable<"#, Unique),
        WeightedPattern::new(r#"ReturnType<"#, Unique),
        WeightedPattern::new(r#"InstanceType<"#, Unique),
        WeightedPattern::new(r#"Promise<[^>]+>"#, Unique),
        WeightedPattern::new(r#"Array<[^>]+>"#, Unique),
        WeightedPattern::new(r#"Map<[^>]+>"#, Unique),
        WeightedPattern::new(r#"Set<[^>]+>"#, Unique),
        WeightedPattern::new(r#"\?\."#, Unique),
        WeightedPattern::new(r#"\?\?"#, Unique),
        WeightedPattern::new(r#"!\."#, Unique),
        WeightedPattern::new(r#"@Injectable\("#, Unique),
        WeightedPattern::new(r#"@Component\("#, Unique),
        WeightedPattern::new(r#"@Module\("#, Unique),
        
        // All JavaScript patterns also apply (8 pts each as Strong)
        WeightedPattern::new(r#"==="#, Strong),
        WeightedPattern::new(r#"!=="#, Strong),
        WeightedPattern::new(r#"=>\s*\{"#, Strong),
        WeightedPattern::new(r#"export\s+default"#, Strong),
        WeightedPattern::new(r#"import\s+.+\s+from"#, Strong),
        WeightedPattern::new(r#"async\s+function"#, Strong),
        WeightedPattern::new(r#"\.then\s*\("#, Strong),
        WeightedPattern::new(r#"\bconst\s+\w+"#, Strong),
        WeightedPattern::new(r#"\blet\s+\w+"#, Strong),
        WeightedPattern::new(r#"\bclass\s+\w+"#, Strong),
        WeightedPattern::new(r#"constructor\s*\("#, Strong),
        
        // Medium indicators
        WeightedPattern::new(r#"\bfunction\s+\w+"#, Medium),
        WeightedPattern::new(r#"//[^\n]+$"#, Medium),
        WeightedPattern::new(r#"/\*[\s\S]*?\*/"#, Medium),
        
        // Negative patterns
        WeightedPattern::new(r#"\bfn\s+\w+"#, Negative),
        WeightedPattern::new(r#"\bdef\s+\w+"#, Negative),
        WeightedPattern::new(r#"\bfunc\s+\w+"#, Negative),
    ],
};

pub static GO: LanguageDefinition = LanguageDefinition {
    name: "go",
    patterns: &[
        // Unique to Go (10 pts each)
        WeightedPattern::new(r#"func\s+\w+\s*\([^)]*\)\s*(\([^)]*\))?\s*\{"#, Unique),
        WeightedPattern::new(r#"func\s*\([^)]+\)\s*\w+"#, Unique),
        WeightedPattern::new(r#"\bpackage\s+\w+"#, Unique),
        WeightedPattern::new(r#"import\s*\("#, Unique),
        WeightedPattern::new(r#"import\s+"[^"]+""#, Unique),
        WeightedPattern::new(r#":="#, Unique),
        WeightedPattern::new(r#"go\s+\w+"#, Unique),
        WeightedPattern::new(r#"\bchan\s+\w+"#, Unique),
        WeightedPattern::new(r#"<-\s*\w+"#, Unique),
        WeightedPattern::new(r#"\w+\s*<-"#, Unique),
        WeightedPattern::new(r#"\bdefer\s+\w+"#, Unique),
        WeightedPattern::new(r#"\bselect\s*\{"#, Unique),
        WeightedPattern::new(r#"\bcase\s+<-"#, Unique),
        WeightedPattern::new(r#"\bstruct\s*\{"#, Unique),
        WeightedPattern::new(r#"\binterface\s*\{"#, Unique),
        WeightedPattern::new(r#"\bmap\[[^\]]+\]"#, Unique),
        WeightedPattern::new(r#"\[\]\w+"#, Unique),
        WeightedPattern::new(r#"\bmake\s*\("#, Unique),
        WeightedPattern::new(r#"fmt\.Print"#, Unique),
        WeightedPattern::new(r#"fmt\.Sprintf"#, Unique),
        WeightedPattern::new(r#"fmt\.Errorf"#, Unique),
        WeightedPattern::new(r#"errors\.New"#, Unique),
        WeightedPattern::new(r#"if\s+err\s*!="#, Unique),
        WeightedPattern::new(r#"return\s+nil\s*,"#, Unique),
        WeightedPattern::new(r#"return\s+\w+,\s*nil"#, Unique),
        WeightedPattern::new(r#"panic\s*\("#, Unique),
        WeightedPattern::new(r#"recover\s*\("#, Unique),
        WeightedPattern::new(r#"context\.Context"#, Unique),
        WeightedPattern::new(r#"ctx\s+context\.Context"#, Unique),
        WeightedPattern::new(r#"\*\w+\.\w+"#, Unique),
        WeightedPattern::new(r#"&\w+\{"#, Unique),
        WeightedPattern::new(r#"\.Error\(\)"#, Unique),
        WeightedPattern::new(r#"json:"[^"]+""#, Unique),
        
        // Strong indicators (8 pts each)
        WeightedPattern::new(r#"\bfunc\s+\w+"#, Strong),
        WeightedPattern::new(r#"\btype\s+\w+\s+struct"#, Strong),
        WeightedPattern::new(r#"\btype\s+\w+\s+interface"#, Strong),
        WeightedPattern::new(r#"\bvar\s+\w+"#, Strong),
        WeightedPattern::new(r#"\bconst\s+\w+"#, Strong),
        WeightedPattern::new(r#"\breturn\s"#, Strong),
        WeightedPattern::new(r#"if\s+\w+"#, Strong),
        WeightedPattern::new(r#"for\s+\w+"#, Strong),
        WeightedPattern::new(r#"range\s+\w+"#, Strong),
        WeightedPattern::new(r#"\bnil\b"#, Strong),
        
        // Medium indicators (5 pts each)
        WeightedPattern::new(r#"//[^\n]+$"#, Medium),
        WeightedPattern::new(r#"/\*[\s\S]*?\*/"#, Medium),
        WeightedPattern::new(r#"\btrue\b"#, Medium),
        WeightedPattern::new(r#"\bfalse\b"#, Medium),
        WeightedPattern::new(r#"\bstring\b"#, Medium),
        WeightedPattern::new(r#"\bint\b"#, Medium),
        WeightedPattern::new(r#"\bint64\b"#, Medium),
        WeightedPattern::new(r#"\bfloat64\b"#, Medium),
        WeightedPattern::new(r#"\bbyte\b"#, Medium),
        WeightedPattern::new(r#"\berror\b"#, Medium),
        
        // Negative patterns
        WeightedPattern::new(r#"\bfn\s+\w+"#, Negative),
        WeightedPattern::new(r#"\bdef\s+\w+"#, Negative),
        WeightedPattern::new(r#"function\s+\w+"#, Negative),
        WeightedPattern::new(r#"public\s+class\s"#, Negative),
        WeightedPattern::new(r##"#\["##, Negative),
        // Shell command patterns - disqualify Go for shell content
        WeightedPattern::new(r#"\bbrew\s+install"#, Negative),
        WeightedPattern::new(r#"\bapt-get\s+"#, Negative),
        WeightedPattern::new(r#"\bcurl\s+-"#, Negative),
        WeightedPattern::new(r#"\bwget\s+"#, Negative),
        WeightedPattern::new(r#"\bgit\s+(clone|pull|push)"#, Negative),
        WeightedPattern::new(r#"\bnpm\s+(install|run)"#, Negative),
        WeightedPattern::new(r#"\byarn\s+(install|add)"#, Negative),
        WeightedPattern::new(r#"\bcargo\s+(build|run)"#, Negative),
        WeightedPattern::new(r#"\bexport\s+\w+=['\"]"#, Negative),
        WeightedPattern::new(r##"#!/bin/(bash|sh)"##, Negative),
    ],
};

pub static JAVA: LanguageDefinition = LanguageDefinition {
    name: "java",
    patterns: &[
        // Unique to Java (10 pts each)
        WeightedPattern::new(r#"public\s+class\s+\w+"#, Unique),
        WeightedPattern::new(r#"public\s+interface\s+\w+"#, Unique),
        WeightedPattern::new(r#"public\s+enum\s+\w+"#, Unique),
        WeightedPattern::new(r#"private\s+static\s+final"#, Unique),
        WeightedPattern::new(r#"public\s+static\s+void\s+main"#, Unique),
        WeightedPattern::new(r#"System\.out\.print"#, Unique),
        WeightedPattern::new(r#"System\.err\.print"#, Unique),
        WeightedPattern::new(r#"@Override\b"#, Unique),
        WeightedPattern::new(r#"@Autowired\b"#, Unique),
        WeightedPattern::new(r#"@Service\b"#, Unique),
        WeightedPattern::new(r#"@Controller\b"#, Unique),
        WeightedPattern::new(r#"@Repository\b"#, Unique),
        WeightedPattern::new(r#"@Bean\b"#, Unique),
        WeightedPattern::new(r#"@Entity\b"#, Unique),
        WeightedPattern::new(r#"@Table\b"#, Unique),
        WeightedPattern::new(r#"@Column\b"#, Unique),
        WeightedPattern::new(r#"@Id\b"#, Unique),
        WeightedPattern::new(r#"@RequestMapping\b"#, Unique),
        WeightedPattern::new(r#"@GetMapping\b"#, Unique),
        WeightedPattern::new(r#"@PostMapping\b"#, Unique),
        WeightedPattern::new(r#"throws\s+\w+Exception"#, Unique),
        WeightedPattern::new(r#"catch\s*\(\s*\w+Exception"#, Unique),
        WeightedPattern::new(r#"new\s+\w+\(\)"#, Unique),
        WeightedPattern::new(r#"ArrayList<"#, Unique),
        WeightedPattern::new(r#"HashMap<"#, Unique),
        WeightedPattern::new(r#"HashSet<"#, Unique),
        WeightedPattern::new(r#"List<"#, Unique),
        WeightedPattern::new(r#"Map<"#, Unique),
        WeightedPattern::new(r#"Set<"#, Unique),
        WeightedPattern::new(r#"Optional<"#, Unique),
        WeightedPattern::new(r#"Stream<"#, Unique),
        WeightedPattern::new(r#"\.stream\(\)"#, Unique),
        WeightedPattern::new(r#"\.collect\("#, Unique),
        WeightedPattern::new(r#"Collectors\."#, Unique),
        WeightedPattern::new(r#"\.orElse\("#, Unique),
        WeightedPattern::new(r#"\.orElseThrow\("#, Unique),
        WeightedPattern::new(r#"\.isPresent\("#, Unique),
        WeightedPattern::new(r#"\.ifPresent\("#, Unique),
        WeightedPattern::new(r#"package\s+[\w.]+;"#, Unique),
        WeightedPattern::new(r#"import\s+[\w.]+;"#, Unique),
        WeightedPattern::new(r#"import\s+static\s"#, Unique),
        WeightedPattern::new(r#"implements\s+\w+"#, Unique),
        WeightedPattern::new(r#"extends\s+\w+"#, Unique),
        
        // Strong indicators (8 pts each)
        WeightedPattern::new(r#"\bpublic\s+\w+"#, Strong),
        WeightedPattern::new(r#"\bprivate\s+\w+"#, Strong),
        WeightedPattern::new(r#"\bprotected\s+\w+"#, Strong),
        WeightedPattern::new(r#"\bstatic\s+\w+"#, Strong),
        WeightedPattern::new(r#"\bfinal\s+\w+"#, Strong),
        WeightedPattern::new(r#"\babstract\s+\w+"#, Strong),
        WeightedPattern::new(r#"\bvoid\s+\w+"#, Strong),
        WeightedPattern::new(r#"\breturn\s"#, Strong),
        WeightedPattern::new(r#"\bthis\.\w+"#, Strong),
        WeightedPattern::new(r#"\bsuper\.\w+"#, Strong),
        WeightedPattern::new(r#"\bnew\s+\w+"#, Strong),
        WeightedPattern::new(r#"try\s*\{"#, Strong),
        WeightedPattern::new(r#"catch\s*\("#, Strong),
        WeightedPattern::new(r#"finally\s*\{"#, Strong),
        
        // Medium indicators (5 pts each)
        WeightedPattern::new(r#"//[^\n]+$"#, Medium),
        WeightedPattern::new(r#"/\*[\s\S]*?\*/"#, Medium),
        WeightedPattern::new(r#"\btrue\b"#, Medium),
        WeightedPattern::new(r#"\bfalse\b"#, Medium),
        WeightedPattern::new(r#"\bnull\b"#, Medium),
        WeightedPattern::new(r#"\bString\b"#, Medium),
        WeightedPattern::new(r#"\bint\b"#, Medium),
        WeightedPattern::new(r#"\blong\b"#, Medium),
        WeightedPattern::new(r#"\bdouble\b"#, Medium),
        WeightedPattern::new(r#"\bboolean\b"#, Medium),
        
        // Negative patterns
        WeightedPattern::new(r#"\bfn\s+\w+"#, Negative),
        WeightedPattern::new(r#"\bdef\s+\w+"#, Negative),
        WeightedPattern::new(r#"\bfunc\s+\w+"#, Negative),
        WeightedPattern::new(r##"#\["##, Negative),
        WeightedPattern::new(r#":="#, Negative),
    ],
};

pub static C_LANG: LanguageDefinition = LanguageDefinition {
    name: "c",
    patterns: &[
        // Unique to C (10 pts each)
        WeightedPattern::new(r##"#include\s*<[\w./]+>"##, Unique),
        WeightedPattern::new(r##"#include\s*"[\w./]+""##, Unique),
        WeightedPattern::new(r##"#define\s+\w+"##, Unique),
        WeightedPattern::new(r##"#ifdef\s+\w+"##, Unique),
        WeightedPattern::new(r##"#ifndef\s+\w+"##, Unique),
        WeightedPattern::new(r##"#endif\b"##, Unique),
        WeightedPattern::new(r##"#pragma\s+\w+"##, Unique),
        WeightedPattern::new(r##"#if\s+defined"##, Unique),
        WeightedPattern::new(r#"int\s+main\s*\("#, Unique),
        WeightedPattern::new(r#"void\s+main\s*\("#, Unique),
        WeightedPattern::new(r#"printf\s*\("#, Unique),
        WeightedPattern::new(r#"scanf\s*\("#, Unique),
        WeightedPattern::new(r#"fprintf\s*\("#, Unique),
        WeightedPattern::new(r#"sprintf\s*\("#, Unique),
        WeightedPattern::new(r#"malloc\s*\("#, Unique),
        WeightedPattern::new(r#"calloc\s*\("#, Unique),
        WeightedPattern::new(r#"realloc\s*\("#, Unique),
        WeightedPattern::new(r#"free\s*\("#, Unique),
        WeightedPattern::new(r#"sizeof\s*\("#, Unique),
        WeightedPattern::new(r#"memcpy\s*\("#, Unique),
        WeightedPattern::new(r#"memset\s*\("#, Unique),
        WeightedPattern::new(r#"strcpy\s*\("#, Unique),
        WeightedPattern::new(r#"strlen\s*\("#, Unique),
        WeightedPattern::new(r#"strcmp\s*\("#, Unique),
        WeightedPattern::new(r#"\bNULL\b"#, Unique),
        WeightedPattern::new(r#"\bstruct\s+\w+\s*\{"#, Unique),
        WeightedPattern::new(r#"\btypedef\s+struct"#, Unique),
        WeightedPattern::new(r#"\btypedef\s+enum"#, Unique),
        WeightedPattern::new(r#"\bunion\s+\w+"#, Unique),
        WeightedPattern::new(r#"\*\w+\s*="#, Unique),
        WeightedPattern::new(r#"&\w+\b"#, Unique),
        WeightedPattern::new(r#"->\w+"#, Unique),
        WeightedPattern::new(r#"\bunsigned\s+(int|char|long)"#, Unique),
        WeightedPattern::new(r#"\bsigned\s+(int|char|long)"#, Unique),
        WeightedPattern::new(r#"\bextern\s+\w+"#, Unique),
        WeightedPattern::new(r#"\bstatic\s+\w+"#, Unique),
        WeightedPattern::new(r#"\bconst\s+\w+"#, Unique),
        WeightedPattern::new(r#"\bvolatile\s+\w+"#, Unique),
        WeightedPattern::new(r#"\bregister\s+\w+"#, Unique),
        
        // Strong indicators (8 pts each)
        WeightedPattern::new(r#"\bint\s+\w+"#, Strong),
        WeightedPattern::new(r#"\bchar\s+\w+"#, Strong),
        WeightedPattern::new(r#"\bfloat\s+\w+"#, Strong),
        WeightedPattern::new(r#"\bdouble\s+\w+"#, Strong),
        WeightedPattern::new(r#"\bvoid\s+\w+"#, Strong),
        WeightedPattern::new(r#"\blong\s+\w+"#, Strong),
        WeightedPattern::new(r#"\bshort\s+\w+"#, Strong),
        WeightedPattern::new(r#"\breturn\s"#, Strong),
        WeightedPattern::new(r#"if\s*\("#, Strong),
        WeightedPattern::new(r#"else\s*\{"#, Strong),
        WeightedPattern::new(r#"for\s*\("#, Strong),
        WeightedPattern::new(r#"while\s*\("#, Strong),
        WeightedPattern::new(r#"switch\s*\("#, Strong),
        WeightedPattern::new(r#"case\s+\w+:"#, Strong),
        
        // Medium indicators (5 pts each)
        WeightedPattern::new(r#"//[^\n]+$"#, Medium),
        WeightedPattern::new(r#"/\*[\s\S]*?\*/"#, Medium),
        WeightedPattern::new(r#"\bbreak\b"#, Medium),
        WeightedPattern::new(r#"\bcontinue\b"#, Medium),
        WeightedPattern::new(r#"\bgoto\s+\w+"#, Medium),
        
        // Negative patterns
        WeightedPattern::new(r#"\bclass\s+\w+"#, Negative),
        WeightedPattern::new(r#"public\s*:"#, Negative),
        WeightedPattern::new(r#"private\s*:"#, Negative),
        WeightedPattern::new(r#"\bnamespace\s+"#, Negative),
        WeightedPattern::new(r#"\btemplate\s*<"#, Negative),
        WeightedPattern::new(r#"\bstd::"#, Negative),
        WeightedPattern::new(r#"\bfn\s+"#, Negative),
        WeightedPattern::new(r#"\bdef\s+"#, Negative),
        WeightedPattern::new(r#"\bfunc\s+"#, Negative),
    ],
};

pub static CPP: LanguageDefinition = LanguageDefinition {
    name: "cpp",
    patterns: &[
        // Unique to C++ (10 pts each)
        WeightedPattern::new(r#"\bstd::\w+"#, Unique),
        WeightedPattern::new(r#"\bnamespace\s+\w+"#, Unique),
        WeightedPattern::new(r#"\busing\s+namespace"#, Unique),
        WeightedPattern::new(r#"\btemplate\s*<"#, Unique),
        WeightedPattern::new(r#"\bclass\s+\w+\s*(:\s*(public|private|protected))?"#, Unique),
        WeightedPattern::new(r#"public\s*:"#, Unique),
        WeightedPattern::new(r#"private\s*:"#, Unique),
        WeightedPattern::new(r#"protected\s*:"#, Unique),
        WeightedPattern::new(r#"\bvirtual\s+\w+"#, Unique),
        WeightedPattern::new(r#"\boverride\b"#, Unique),
        WeightedPattern::new(r#"\bfinal\b"#, Unique),
        WeightedPattern::new(r#"\bconst\s+\w+\s*&"#, Unique),
        WeightedPattern::new(r#"\w+&&"#, Unique),
        WeightedPattern::new(r#"\bauto\s+\w+"#, Unique),
        WeightedPattern::new(r#"\bdecltype\s*\("#, Unique),
        WeightedPattern::new(r#"\bconstexpr\s+"#, Unique),
        WeightedPattern::new(r#"\bconsteval\s+"#, Unique),
        WeightedPattern::new(r#"\bconcept\s+"#, Unique),
        WeightedPattern::new(r#"\brequires\s+"#, Unique),
        WeightedPattern::new(r#"\blambda\s*\["#, Unique),
        WeightedPattern::new(r#"\[\s*[=&]?\s*\]\s*\("#, Unique),
        WeightedPattern::new(r#"std::vector<"#, Unique),
        WeightedPattern::new(r#"std::string"#, Unique),
        WeightedPattern::new(r#"std::map<"#, Unique),
        WeightedPattern::new(r#"std::unordered_map<"#, Unique),
        WeightedPattern::new(r#"std::set<"#, Unique),
        WeightedPattern::new(r#"std::unique_ptr<"#, Unique),
        WeightedPattern::new(r#"std::shared_ptr<"#, Unique),
        WeightedPattern::new(r#"std::make_unique<"#, Unique),
        WeightedPattern::new(r#"std::make_shared<"#, Unique),
        WeightedPattern::new(r#"std::move\("#, Unique),
        WeightedPattern::new(r#"std::forward<"#, Unique),
        WeightedPattern::new(r#"std::cout"#, Unique),
        WeightedPattern::new(r#"std::cin"#, Unique),
        WeightedPattern::new(r#"std::endl"#, Unique),
        WeightedPattern::new(r#"<<\s*std::"#, Unique),
        WeightedPattern::new(r#"::\w+\s*\("#, Unique),
        WeightedPattern::new(r#"\bnew\s+\w+\("#, Unique),
        WeightedPattern::new(r#"\bdelete\s+\w+"#, Unique),
        WeightedPattern::new(r#"\bdelete\[\]\s*\w+"#, Unique),
        WeightedPattern::new(r#"nullptr\b"#, Unique),
        WeightedPattern::new(r#"dynamic_cast<"#, Unique),
        WeightedPattern::new(r#"static_cast<"#, Unique),
        WeightedPattern::new(r#"reinterpret_cast<"#, Unique),
        WeightedPattern::new(r#"const_cast<"#, Unique),
        WeightedPattern::new(r#"\bthrow\s+"#, Unique),
        WeightedPattern::new(r#"catch\s*\(\s*\w+"#, Unique),
        WeightedPattern::new(r##"#include\s*<iostream>"##, Unique),
        WeightedPattern::new(r##"#include\s*<vector>"##, Unique),
        WeightedPattern::new(r##"#include\s*<string>"##, Unique),
        WeightedPattern::new(r##"#include\s*<memory>"##, Unique),
        WeightedPattern::new(r##"#include\s*<algorithm>"##, Unique),
        
        // Strong indicators (8 pts - also in C)
        WeightedPattern::new(r##"#include\s*<[\w./]+>"##, Strong),
        WeightedPattern::new(r##"#define\s+\w+"##, Strong),
        WeightedPattern::new(r#"\bint\s+\w+"#, Strong),
        WeightedPattern::new(r#"\bvoid\s+\w+"#, Strong),
        WeightedPattern::new(r#"\breturn\s"#, Strong),
        WeightedPattern::new(r#"if\s*\("#, Strong),
        WeightedPattern::new(r#"for\s*\("#, Strong),
        WeightedPattern::new(r#"while\s*\("#, Strong),
        
        // Medium indicators (5 pts each)
        WeightedPattern::new(r#"//[^\n]+$"#, Medium),
        WeightedPattern::new(r#"/\*[\s\S]*?\*/"#, Medium),
        WeightedPattern::new(r#"\btrue\b"#, Medium),
        WeightedPattern::new(r#"\bfalse\b"#, Medium),
        WeightedPattern::new(r#"\bNULL\b"#, Medium),
        
        // Negative patterns
        WeightedPattern::new(r#"\bfn\s+"#, Negative),
        WeightedPattern::new(r#"\bdef\s+"#, Negative),
        WeightedPattern::new(r#"\bfunc\s+"#, Negative),
        WeightedPattern::new(r#"public\s+class\s"#, Negative),
        WeightedPattern::new(r#"package\s+[\w.]+"#, Negative),
        // Shell command patterns - disqualify CPP for shell content
        WeightedPattern::new(r#"\bbrew\s+install"#, Negative),
        WeightedPattern::new(r#"\bapt-get\s+"#, Negative),
        WeightedPattern::new(r#"\bcurl\s+-"#, Negative),
        WeightedPattern::new(r#"\bwget\s+"#, Negative),
        WeightedPattern::new(r#"\bgit\s+(clone|pull|push)"#, Negative),
        WeightedPattern::new(r#"\bnpm\s+(install|run)"#, Negative),
        WeightedPattern::new(r#"\byarn\s+(install|add)"#, Negative),
        WeightedPattern::new(r#"\bcargo\s+(build|run)"#, Negative),
        WeightedPattern::new(r#"\bexport\s+\w+=['\"]"#, Negative),
        WeightedPattern::new(r##"#!/bin/(bash|sh)"##, Negative),
    ],
};

pub static RUBY: LanguageDefinition = LanguageDefinition {
    name: "ruby",
    patterns: &[
        // Unique to Ruby (10 pts each)
        WeightedPattern::new(r#"def\s+\w+(\s*\([^)]*\))?\s*$"#, Unique),
        WeightedPattern::new(r#"\bend\s*$"#, Unique),
        WeightedPattern::new(r#"class\s+\w+\s*(<\s*\w+)?\s*$"#, Unique),
        WeightedPattern::new(r#"module\s+\w+\s*$"#, Unique),
        WeightedPattern::new(r#"require\s+['"]"#, Unique),
        WeightedPattern::new(r#"require_relative\s+['"]"#, Unique),
        WeightedPattern::new(r#"attr_accessor\s+:\w+"#, Unique),
        WeightedPattern::new(r#"attr_reader\s+:\w+"#, Unique),
        WeightedPattern::new(r#"attr_writer\s+:\w+"#, Unique),
        WeightedPattern::new(r#":\w+\s*=>"#, Unique),
        WeightedPattern::new(r#"\w+:\s*\w+"#, Unique),
        WeightedPattern::new(r##"#\{[^}]+\}"##, Unique),
        WeightedPattern::new(r#"@\w+\s*="#, Unique),
        WeightedPattern::new(r#"@@\w+\s*="#, Unique),
        WeightedPattern::new(r#"\$\w+\s*="#, Unique),
        WeightedPattern::new(r#"\.each\s+do\s*\|"#, Unique),
        WeightedPattern::new(r#"\.map\s+do\s*\|"#, Unique),
        WeightedPattern::new(r#"\.select\s+do\s*\|"#, Unique),
        WeightedPattern::new(r#"\.reject\s+do\s*\|"#, Unique),
        WeightedPattern::new(r#"\.reduce\s+do\s*\|"#, Unique),
        WeightedPattern::new(r#"\|\w+\|"#, Unique),
        WeightedPattern::new(r#"do\s*\|[^|]+\|"#, Unique),
        WeightedPattern::new(r#"\{\s*\|[^|]+\|"#, Unique),
        WeightedPattern::new(r#"puts\s+"#, Unique),
        WeightedPattern::new(r#"gets\s*"#, Unique),
        WeightedPattern::new(r#"p\s+\w+"#, Unique),
        WeightedPattern::new(r#"\.nil\?"#, Unique),
        WeightedPattern::new(r#"\.empty\?"#, Unique),
        WeightedPattern::new(r#"\.present\?"#, Unique),
        WeightedPattern::new(r#"\.blank\?"#, Unique),
        WeightedPattern::new(r#"\w+\?"#, Unique),
        WeightedPattern::new(r#"\w+!"#, Unique),
        WeightedPattern::new(r#"unless\s+"#, Unique),
        WeightedPattern::new(r#"until\s+"#, Unique),
        WeightedPattern::new(r#"begin\s*$"#, Unique),
        WeightedPattern::new(r#"rescue\s+"#, Unique),
        WeightedPattern::new(r#"raise\s+"#, Unique),
        WeightedPattern::new(r#"ensure\s*$"#, Unique),
        WeightedPattern::new(r#"yield\s*"#, Unique),
        WeightedPattern::new(r#"&block\)"#, Unique),
        WeightedPattern::new(r#"\*args\)"#, Unique),
        WeightedPattern::new(r#"\*\*kwargs\)"#, Unique),
        WeightedPattern::new(r#"lambda\s*\{"#, Unique),
        WeightedPattern::new(r#"->\s*\{"#, Unique),
        WeightedPattern::new(r#"proc\s*\{"#, Unique),
        
        // Strong indicators (8 pts each)
        WeightedPattern::new(r#"\bdef\s+\w+"#, Strong),
        WeightedPattern::new(r#"\bclass\s+\w+"#, Strong),
        WeightedPattern::new(r#"\bmodule\s+\w+"#, Strong),
        WeightedPattern::new(r#"\bif\s+\w+"#, Strong),
        WeightedPattern::new(r#"\belsif\s+"#, Strong),
        WeightedPattern::new(r#"\belse\s*$"#, Strong),
        WeightedPattern::new(r#"\bwhile\s+"#, Strong),
        WeightedPattern::new(r#"\bfor\s+\w+\s+in\s"#, Strong),
        WeightedPattern::new(r#"\breturn\s"#, Strong),
        WeightedPattern::new(r##"#[^\n{]+$"##, Strong),
        
        // Medium indicators (5 pts each)
        WeightedPattern::new(r#"\btrue\b"#, Medium),
        WeightedPattern::new(r#"\bfalse\b"#, Medium),
        WeightedPattern::new(r#"\bnil\b"#, Medium),
        WeightedPattern::new(r#"\band\b"#, Medium),
        WeightedPattern::new(r#"\bor\b"#, Medium),
        WeightedPattern::new(r#"\bnot\b"#, Medium),
        
        // Negative patterns
        WeightedPattern::new(r#"\bfn\s+"#, Negative),
        WeightedPattern::new(r#"\bfunc\s+"#, Negative),
        WeightedPattern::new(r#"function\s+"#, Negative),
        WeightedPattern::new(r#"public\s+class"#, Negative),
        WeightedPattern::new(r##"#\["##, Negative),
        WeightedPattern::new(r#"^---\s*$"#, Negative),       // YAML document start
        WeightedPattern::new(r#"apiVersion:\s+"#, Negative), // Kubernetes YAML
        WeightedPattern::new(r#"kind:\s+"#, Negative),       // Kubernetes YAML
        WeightedPattern::new(r#"metadata:\s*$"#, Negative),  // Kubernetes YAML
        WeightedPattern::new(r#"spec:\s*$"#, Negative),      // Kubernetes YAML
        // GitHub Actions / GitLab CI YAML patterns (disqualify Ruby)
        WeightedPattern::new(r#"on:\s*$"#, Negative),             // GitHub Actions trigger
        WeightedPattern::new(r#"jobs:\s*$"#, Negative),           // GitHub Actions jobs
        WeightedPattern::new(r#"steps:\s*$"#, Negative),          // Workflow steps
        WeightedPattern::new(r#"runs-on:\s+"#, Negative),         // Runner specification
        WeightedPattern::new(r#"uses:\s+[^@]+@"#, Negative),      // Action usage
        WeightedPattern::new(r#"stages:\s*$"#, Negative),         // GitLab CI stages
        WeightedPattern::new(r#"script:\s*$"#, Negative),         // GitLab CI script
        WeightedPattern::new(r#"pull_request:\s*$"#, Negative),   // PR trigger
        WeightedPattern::new(r#"issue_comment:\s*$"#, Negative),  // Issue comment trigger
        WeightedPattern::new(r#"workflow_dispatch:\s*$"#, Negative), // Manual trigger
        // Shell command patterns - disqualify Ruby for shell content
        WeightedPattern::new(r#"\bbrew\s+install"#, Negative),
        WeightedPattern::new(r#"\bapt-get\s+"#, Negative),
        WeightedPattern::new(r#"\bcurl\s+-"#, Negative),
        WeightedPattern::new(r#"\bwget\s+"#, Negative),
        WeightedPattern::new(r#"\bgit\s+(clone|pull|push)"#, Negative),
        WeightedPattern::new(r#"\bnpm\s+(install|run)"#, Negative),
        WeightedPattern::new(r#"\byarn\s+(install|add)"#, Negative),
        WeightedPattern::new(r#"\bcargo\s+(build|run)"#, Negative),
        WeightedPattern::new(r#"\bexport\s+\w+=['\"]"#, Negative),
        WeightedPattern::new(r##"#!/bin/(bash|sh)"##, Negative),
    ],
};

pub static PHP: LanguageDefinition = LanguageDefinition {
    name: "php",
    patterns: &[
        // Unique to PHP (10 pts each)
        WeightedPattern::new(r#"<\?php"#, Unique),
        WeightedPattern::new(r#"\?>"#, Unique),
        WeightedPattern::new(r#"\$\w+\s*="#, Unique),
        WeightedPattern::new(r#"\$\w+->"#, Unique),
        WeightedPattern::new(r#"\$this->"#, Unique),
        WeightedPattern::new(r#"self::"#, Unique),
        WeightedPattern::new(r#"parent::"#, Unique),
        WeightedPattern::new(r#"::\$\w+"#, Unique),
        WeightedPattern::new(r#"function\s+\w+\s*\([^)]*\)"#, Unique),
        WeightedPattern::new(r#"public\s+function"#, Unique),
        WeightedPattern::new(r#"private\s+function"#, Unique),
        WeightedPattern::new(r#"protected\s+function"#, Unique),
        WeightedPattern::new(r#"class\s+\w+\s+(extends|implements)"#, Unique),
        WeightedPattern::new(r#"\bnamespace\s+[\w\\]+"#, Unique),
        WeightedPattern::new(r#"\buse\s+[\w\\]+"#, Unique),
        WeightedPattern::new(r#"require_once\s+"#, Unique),
        WeightedPattern::new(r#"include_once\s+"#, Unique),
        WeightedPattern::new(r#"\brequire\s+"#, Unique),
        WeightedPattern::new(r#"\binclude\s+"#, Unique),
        WeightedPattern::new(r#"echo\s+"#, Unique),
        WeightedPattern::new(r#"print\s+"#, Unique),
        WeightedPattern::new(r#"var_dump\s*\("#, Unique),
        WeightedPattern::new(r#"print_r\s*\("#, Unique),
        WeightedPattern::new(r#"isset\s*\("#, Unique),
        WeightedPattern::new(r#"empty\s*\("#, Unique),
        WeightedPattern::new(r#"array\s*\("#, Unique),
        WeightedPattern::new(r#"\[.*=>"#, Unique),
        WeightedPattern::new(r#"=>"#, Unique),
        WeightedPattern::new(r#"\bnew\s+\w+"#, Unique),
        WeightedPattern::new(r#"__construct\s*\("#, Unique),
        WeightedPattern::new(r#"__destruct\s*\("#, Unique),
        WeightedPattern::new(r#"__get\s*\("#, Unique),
        WeightedPattern::new(r#"__set\s*\("#, Unique),
        WeightedPattern::new(r#"__call\s*\("#, Unique),
        WeightedPattern::new(r#"__toString\s*\("#, Unique),
        
        // Strong indicators (8 pts each)
        WeightedPattern::new(r#"\bfunction\s+"#, Strong),
        WeightedPattern::new(r#"\bclass\s+"#, Strong),
        WeightedPattern::new(r#"\binterface\s+"#, Strong),
        WeightedPattern::new(r#"\btrait\s+"#, Strong),
        WeightedPattern::new(r#"\bpublic\s+"#, Strong),
        WeightedPattern::new(r#"\bprivate\s+"#, Strong),
        WeightedPattern::new(r#"\bprotected\s+"#, Strong),
        WeightedPattern::new(r#"\breturn\s"#, Strong),
        WeightedPattern::new(r#"if\s*\("#, Strong),
        WeightedPattern::new(r#"foreach\s*\("#, Strong),
        WeightedPattern::new(r#"while\s*\("#, Strong),
        WeightedPattern::new(r#"for\s*\("#, Strong),
        
        // Medium indicators (5 pts each)
        WeightedPattern::new(r#"//[^\n]+$"#, Medium),
        WeightedPattern::new(r#"/\*[\s\S]*?\*/"#, Medium),
        WeightedPattern::new(r#"\btrue\b"#, Medium),
        WeightedPattern::new(r#"\bfalse\b"#, Medium),
        WeightedPattern::new(r#"\bnull\b"#, Medium),
        
        // Negative patterns
        WeightedPattern::new(r#"\bfn\s+"#, Negative),
        WeightedPattern::new(r#"\bdef\s+"#, Negative),
        WeightedPattern::new(r#"\bfunc\s+"#, Negative),
        WeightedPattern::new(r##"#\["##, Negative),
        // Shell command patterns - disqualify PHP for shell content
        WeightedPattern::new(r#"\bbrew\s+install"#, Negative),
        WeightedPattern::new(r#"\bapt-get\s+"#, Negative),
        WeightedPattern::new(r#"\bcurl\s+-"#, Negative),
        WeightedPattern::new(r#"\bwget\s+"#, Negative),
        WeightedPattern::new(r#"\bgit\s+(clone|pull|push)"#, Negative),
        WeightedPattern::new(r#"\bnpm\s+(install|run)"#, Negative),
        WeightedPattern::new(r#"\byarn\s+(install|add)"#, Negative),
        WeightedPattern::new(r#"\bcargo\s+(build|run)"#, Negative),
        WeightedPattern::new(r#"\bexport\s+\w+=['\"]"#, Negative),
        WeightedPattern::new(r##"#!/bin/(bash|sh)"##, Negative),
    ],
};

pub static SHELL: LanguageDefinition = LanguageDefinition {
    name: "bash",
    patterns: &[
        // Unique to Shell/Bash (10 pts each)
        WeightedPattern::new(r##"#!/bin/bash"##, Unique),
        WeightedPattern::new(r##"#!/bin/sh"##, Unique),
        WeightedPattern::new(r##"#!/usr/bin/env\s+bash"##, Unique),
        WeightedPattern::new(r##"#!/usr/bin/env\s+sh"##, Unique),
        WeightedPattern::new(r#"\$\{\w+\}"#, Unique),
        WeightedPattern::new(r#"\$\(\w+"#, Unique),
        WeightedPattern::new(r#"`[^`]+`"#, Unique),
        WeightedPattern::new(r#"\[\[\s+[^\]]+\s+\]\]"#, Unique),
        WeightedPattern::new(r#"\[\s+[^\]]+\s+\]"#, Unique),
        WeightedPattern::new(r#"-eq\b"#, Unique),
        WeightedPattern::new(r#"-ne\b"#, Unique),
        WeightedPattern::new(r#"-lt\b"#, Unique),
        WeightedPattern::new(r#"-gt\b"#, Unique),
        WeightedPattern::new(r#"-le\b"#, Unique),
        WeightedPattern::new(r#"-ge\b"#, Unique),
        WeightedPattern::new(r#"-z\s+"#, Unique),
        WeightedPattern::new(r#"-n\s+"#, Unique),
        WeightedPattern::new(r#"-f\s+"#, Unique),
        WeightedPattern::new(r#"-d\s+"#, Unique),
        WeightedPattern::new(r#"-e\s+"#, Unique),
        WeightedPattern::new(r#"-r\s+"#, Unique),
        WeightedPattern::new(r#"-w\s+"#, Unique),
        WeightedPattern::new(r#"-x\s+"#, Unique),
        WeightedPattern::new(r#"function\s+\w+\s*\(\)\s*\{"#, Unique),
        WeightedPattern::new(r#"\w+\s*\(\)\s*\{"#, Unique),
        WeightedPattern::new(r#"\bfi\b"#, Unique),
        WeightedPattern::new(r#"\bdo\b"#, Unique),
        WeightedPattern::new(r#"\bdone\b"#, Unique),
        WeightedPattern::new(r#"\bthen\b"#, Unique),
        WeightedPattern::new(r#"\besac\b"#, Unique),
        WeightedPattern::new(r#"case\s+\$\w+\s+in"#, Unique),
        WeightedPattern::new(r#"for\s+\w+\s+in\s"#, Unique),
        WeightedPattern::new(r#"while\s+read"#, Unique),
        WeightedPattern::new(r#"\bshift\b"#, Unique),
        WeightedPattern::new(r#"\bexit\s+\d+"#, Unique),
        WeightedPattern::new(r#"export\s+\w+="#, Unique),
        WeightedPattern::new(r#"source\s+"#, Unique),
        WeightedPattern::new(r#"\.\s+\w+"#, Unique),
        WeightedPattern::new(r#"\becho\s+"#, Unique),
        WeightedPattern::new(r#"\bprintf\s+"#, Unique),
        WeightedPattern::new(r#"\bread\s+"#, Unique),
        WeightedPattern::new(r#"local\s+\w+="#, Unique),
        WeightedPattern::new(r#"\bset\s+-[ex]"#, Unique),
        WeightedPattern::new(r#"\|"#, Unique),
        WeightedPattern::new(r#"2>&1"#, Unique),
        WeightedPattern::new(r#">/dev/null"#, Unique),
        WeightedPattern::new(r#"<<\s*\w+"#, Unique),
        WeightedPattern::new(r#"grep\s+"#, Unique),
        WeightedPattern::new(r#"sed\s+"#, Unique),
        WeightedPattern::new(r#"awk\s+"#, Unique),
        WeightedPattern::new(r#"cut\s+"#, Unique),
        WeightedPattern::new(r#"sort\s+"#, Unique),
        WeightedPattern::new(r#"uniq\s+"#, Unique),
        WeightedPattern::new(r#"xargs\s+"#, Unique),
        WeightedPattern::new(r#"find\s+"#, Unique),
        
        // Strong indicators (8 pts each)
        WeightedPattern::new(r#"\bbrew\s*(install|update|upgrade|uninstall|search)"#, Strong),
        WeightedPattern::new(r#"\bapt-get\s*(install|update|upgrade|remove)"#, Strong),
        WeightedPattern::new(r#"\bapt\s*(install|update|upgrade|remove)"#, Strong),
        WeightedPattern::new(r#"\byum\s*(install|update|remove)"#, Strong),
        WeightedPattern::new(r#"\bcurl\s*-"#, Strong),
        WeightedPattern::new(r#"\bwget\s*(http|ftp|--)"#, Strong),
        WeightedPattern::new(r#"\bgit\s*(clone|pull|push|commit|add|status|checkout)"#, Strong),
        WeightedPattern::new(r#"\bnpm\s*(install|run|start|build|test|init)"#, Strong),
        WeightedPattern::new(r#"\byarn\s*(install|add|run|build)"#, Strong),
        WeightedPattern::new(r#"\bpnpm\s*(install|add|run)"#, Strong),
        WeightedPattern::new(r#"\bcargo\s*(build|run|test|install|new)"#, Strong),
        WeightedPattern::new(r#"\bif\s+"#, Strong),
        WeightedPattern::new(r#"\belif\s+"#, Strong),
        WeightedPattern::new(r#"\belse\b"#, Strong),
        WeightedPattern::new(r#"\bwhile\s+"#, Strong),
        WeightedPattern::new(r#"\bfor\s+"#, Strong),
        WeightedPattern::new(r#"\breturn\s+"#, Strong),
        WeightedPattern::new(r##"#[^\n!]+$"##, Strong),
        
        // Medium indicators (5 pts each)
        WeightedPattern::new(r#"\btrue\b"#, Medium),
        WeightedPattern::new(r#"\bfalse\b"#, Medium),
        WeightedPattern::new(r#"\$\d+"#, Medium),
        WeightedPattern::new(r#"\$@"#, Medium),
        WeightedPattern::new(r#"\$\*"#, Medium),
        WeightedPattern::new(r#"\$\?"#, Medium),
        WeightedPattern::new(r#"\$\$"#, Medium),
        
        // Negative patterns
        WeightedPattern::new(r#"\bfn\s+"#, Negative),
        WeightedPattern::new(r#"\bdef\s+"#, Negative),
        WeightedPattern::new(r#"\bfunc\s+"#, Negative),
        WeightedPattern::new(r#"public\s+class"#, Negative),
        WeightedPattern::new(r##"#\["##, Negative),
    ],
};

pub static SQL: LanguageDefinition = LanguageDefinition {
    name: "sql",
    patterns: &[
        // Unique to SQL (10 pts each)
        WeightedPattern::new(r#"(?i)SELECT\s+.+\s+FROM\s+"#, Unique),
        WeightedPattern::new(r#"(?i)INSERT\s+INTO\s+"#, Unique),
        WeightedPattern::new(r#"(?i)UPDATE\s+\w+\s+SET\s+"#, Unique),
        WeightedPattern::new(r#"(?i)DELETE\s+FROM\s+"#, Unique),
        WeightedPattern::new(r#"(?i)CREATE\s+TABLE\s+"#, Unique),
        WeightedPattern::new(r#"(?i)ALTER\s+TABLE\s+"#, Unique),
        WeightedPattern::new(r#"(?i)DROP\s+TABLE\s+"#, Unique),
        WeightedPattern::new(r#"(?i)CREATE\s+INDEX\s+"#, Unique),
        WeightedPattern::new(r#"(?i)CREATE\s+VIEW\s+"#, Unique),
        WeightedPattern::new(r#"(?i)CREATE\s+PROCEDURE\s+"#, Unique),
        WeightedPattern::new(r#"(?i)CREATE\s+FUNCTION\s+"#, Unique),
        WeightedPattern::new(r#"(?i)CREATE\s+TRIGGER\s+"#, Unique),
        WeightedPattern::new(r#"(?i)INNER\s+JOIN\s+"#, Unique),
        WeightedPattern::new(r#"(?i)LEFT\s+JOIN\s+"#, Unique),
        WeightedPattern::new(r#"(?i)RIGHT\s+JOIN\s+"#, Unique),
        WeightedPattern::new(r#"(?i)FULL\s+JOIN\s+"#, Unique),
        WeightedPattern::new(r#"(?i)CROSS\s+JOIN\s+"#, Unique),
        WeightedPattern::new(r#"(?i)ON\s+\w+\.\w+\s*="#, Unique),
        WeightedPattern::new(r#"(?i)WHERE\s+"#, Unique),
        WeightedPattern::new(r#"(?i)GROUP\s+BY\s+"#, Unique),
        WeightedPattern::new(r#"(?i)HAVING\s+"#, Unique),
        WeightedPattern::new(r#"(?i)ORDER\s+BY\s+"#, Unique),
        WeightedPattern::new(r#"(?i)LIMIT\s+\d+"#, Unique),
        WeightedPattern::new(r#"(?i)OFFSET\s+\d+"#, Unique),
        WeightedPattern::new(r#"(?i)DISTINCT\s+"#, Unique),
        WeightedPattern::new(r#"(?i)AS\s+\w+"#, Unique),
        WeightedPattern::new(r#"(?i)COUNT\s*\("#, Unique),
        WeightedPattern::new(r#"(?i)SUM\s*\("#, Unique),
        WeightedPattern::new(r#"(?i)AVG\s*\("#, Unique),
        WeightedPattern::new(r#"(?i)MAX\s*\("#, Unique),
        WeightedPattern::new(r#"(?i)MIN\s*\("#, Unique),
        WeightedPattern::new(r#"(?i)COALESCE\s*\("#, Unique),
        WeightedPattern::new(r#"(?i)CASE\s+WHEN\s+"#, Unique),
        WeightedPattern::new(r#"(?i)THEN\s+"#, Unique),
        WeightedPattern::new(r#"(?i)ELSE\s+"#, Unique),
        WeightedPattern::new(r#"(?i)END\s*$"#, Unique),
        WeightedPattern::new(r#"(?i)PRIMARY\s+KEY"#, Unique),
        WeightedPattern::new(r#"(?i)FOREIGN\s+KEY"#, Unique),
        WeightedPattern::new(r#"(?i)REFERENCES\s+"#, Unique),
        WeightedPattern::new(r#"(?i)NOT\s+NULL"#, Unique),
        WeightedPattern::new(r#"(?i)UNIQUE\s+"#, Unique),
        WeightedPattern::new(r#"(?i)DEFAULT\s+"#, Unique),
        WeightedPattern::new(r#"(?i)AUTO_INCREMENT"#, Unique),
        WeightedPattern::new(r#"(?i)SERIAL\b"#, Unique),
        WeightedPattern::new(r#"(?i)VARCHAR\s*\("#, Unique),
        WeightedPattern::new(r#"(?i)INTEGER\b"#, Unique),
        WeightedPattern::new(r#"(?i)BOOLEAN\b"#, Unique),
        WeightedPattern::new(r#"(?i)TIMESTAMP\b"#, Unique),
        WeightedPattern::new(r#"(?i)BEGIN\s+TRANSACTION"#, Unique),
        WeightedPattern::new(r#"(?i)COMMIT\s*;"#, Unique),
        WeightedPattern::new(r#"(?i)ROLLBACK\s*;"#, Unique),
        
        // Strong indicators (8 pts each)
        WeightedPattern::new(r#"(?i)SELECT\s+"#, Strong),
        WeightedPattern::new(r#"(?i)FROM\s+"#, Strong),
        WeightedPattern::new(r#"(?i)AND\s+"#, Strong),
        WeightedPattern::new(r#"(?i)OR\s+"#, Strong),
        WeightedPattern::new(r#"(?i)IN\s*\("#, Strong),
        WeightedPattern::new(r#"(?i)LIKE\s+"#, Strong),
        WeightedPattern::new(r#"(?i)BETWEEN\s+"#, Strong),
        WeightedPattern::new(r#"(?i)IS\s+NULL"#, Strong),
        WeightedPattern::new(r#"(?i)IS\s+NOT\s+NULL"#, Strong),
        
        // Medium indicators (5 pts each)
        WeightedPattern::new(r#"--[^\n]+$"#, Medium),
        WeightedPattern::new(r#"/\*[\s\S]*?\*/"#, Medium),
        WeightedPattern::new(r#"(?i)NULL\b"#, Medium),
        WeightedPattern::new(r#"(?i)TRUE\b"#, Medium),
        WeightedPattern::new(r#"(?i)FALSE\b"#, Medium),
        
        // Negative patterns
        WeightedPattern::new(r#"\bfn\s+"#, Negative),
        WeightedPattern::new(r#"\bdef\s+"#, Negative),
        WeightedPattern::new(r#"\bfunc\s+"#, Negative),
        WeightedPattern::new(r#"function\s+"#, Negative),
        WeightedPattern::new(r##"#\["##, Negative),
        // Shell command patterns - disqualify SQL for shell content
        WeightedPattern::new(r#"\bbrew\s+install"#, Negative),
        WeightedPattern::new(r#"\bapt-get\s+"#, Negative),
        WeightedPattern::new(r#"\bcurl\s+-"#, Negative),
        WeightedPattern::new(r#"\bwget\s+"#, Negative),
        WeightedPattern::new(r#"\bgit\s+(clone|pull|push)"#, Negative),
        WeightedPattern::new(r#"\bnpm\s+(install|run)"#, Negative),
        WeightedPattern::new(r#"\byarn\s+(install|add)"#, Negative),
        WeightedPattern::new(r#"\bcargo\s+(build|run)"#, Negative),
        WeightedPattern::new(r#"\bexport\s+\w+=['\"]"#, Negative),
        WeightedPattern::new(r##"#!/bin/(bash|sh)"##, Negative),
    ],
};

pub static POWERSHELL: LanguageDefinition = LanguageDefinition {
    name: "powershell",
    patterns: &[
        // Unique to PowerShell (10 pts each) - Cmdlet verb-noun pattern
        WeightedPattern::new(r#"\b(Get|Set|New|Remove|Add|Clear|Copy|Move|Rename|Write|Read|Test|Invoke|Start|Stop|Restart|Enable|Disable|Import|Export|ConvertTo|ConvertFrom|Select|Where|ForEach|Sort|Group|Measure)-[A-Z]\w+"#, Unique),
        
        // PowerShell cmdlet aliases (10 pts each)
        WeightedPattern::new(r#"\b(irm|iex|iwr|iwk)\b"#, Unique),  // Invoke-RestMethod, Invoke-Expression, Invoke-WebRequest, Invoke-WebKit
        
        // .NET type syntax - unique to PowerShell (10 pts each)
        WeightedPattern::new(r#"\[[A-Z]\w+(\.[A-Z]\w+)*\]::"#, Unique),  // [Type]::Method or [Namespace.Type]::
        WeightedPattern::new(r#"\[scriptblock\]::"#, Unique),
        WeightedPattern::new(r#"\[System\.\w+\]::"#, Unique),
        WeightedPattern::new(r#"\[PSCustomObject\]"#, Unique),
        WeightedPattern::new(r#"\[CmdletBinding\(\)"#, Unique),
        WeightedPattern::new(r#"\[Parameter\([^\)]*\)\]"#, Unique),
        WeightedPattern::new(r#"\[ValidateSet\([^\)]*\)\]"#, Unique),
        WeightedPattern::new(r#"\[ValidateNotNull\(\)\]"#, Unique),
        WeightedPattern::new(r#"\[ValidateNotNullOrEmpty\(\)\]"#, Unique),
        
        // PowerShell automatic variables (10 pts each)
        WeightedPattern::new(r#"\$PSScriptRoot\b"#, Unique),
        WeightedPattern::new(r#"\$PSVersionTable\b"#, Unique),
        WeightedPattern::new(r#"\$PSCommandPath\b"#, Unique),
        WeightedPattern::new(r#"\$PSBoundParameters\b"#, Unique),
        WeightedPattern::new(r#"\$ErrorActionPreference\b"#, Unique),
        WeightedPattern::new(r#"\$ProgressPreference\b"#, Unique),
        WeightedPattern::new(r#"\$VerbosePreference\b"#, Unique),
        WeightedPattern::new(r#"\$WarningPreference\b"#, Unique),
        WeightedPattern::new(r#"\$Host\b"#, Unique),
        WeightedPattern::new(r#"\$MyInvocation\b"#, Unique),
        
        // PowerShell comparison operators (10 pts each)
        WeightedPattern::new(r#"-eq\b"#, Unique),
        WeightedPattern::new(r#"-ne\b"#, Unique),
        WeightedPattern::new(r#"-gt\b"#, Unique),
        WeightedPattern::new(r#"-ge\b"#, Unique),
        WeightedPattern::new(r#"-lt\b"#, Unique),
        WeightedPattern::new(r#"-le\b"#, Unique),
        WeightedPattern::new(r#"-like\b"#, Unique),
        WeightedPattern::new(r#"-notlike\b"#, Unique),
        WeightedPattern::new(r#"-match\b"#, Unique),
        WeightedPattern::new(r#"-notmatch\b"#, Unique),
        WeightedPattern::new(r#"-contains\b"#, Unique),
        WeightedPattern::new(r#"-notcontains\b"#, Unique),
        WeightedPattern::new(r#"-in\b"#, Unique),
        WeightedPattern::new(r#"-notin\b"#, Unique),
        
        // PowerShell logical operators (10 pts each)
        WeightedPattern::new(r#"-and\b"#, Unique),
        WeightedPattern::new(r#"-or\b"#, Unique),
        WeightedPattern::new(r#"-not\b"#, Unique),
        WeightedPattern::new(r#"-xor\b"#, Unique),
        
        // PowerShell pipeline variable (10 pts each)
        WeightedPattern::new(r#"\$_\."#, Unique),
        WeightedPattern::new(r#"\$PSItem\."#, Unique),
        
        // PowerShell splatting (10 pts each)
        WeightedPattern::new(r#"@\w+\s*="#, Unique),  // @Parameters = @{}
        WeightedPattern::new(r#"@\{[^}]*\}"#, Unique),  // Hash table literal
        WeightedPattern::new(r#"@\([^\)]*\)"#, Unique),  // Array subexpression
        
        // PowerShell here-strings (10 pts each)
        WeightedPattern::new(r#"@["']"#, Unique),  // Start of here-string
        WeightedPattern::new(r#"^["']@"#, Unique),  // End of here-string
        
        // PowerShell function syntax (8 pts each - Strong)
        WeightedPattern::new(r#"\bfunction\s+\w+\s*\{"#, Strong),
        WeightedPattern::new(r#"\bparam\s*\("#, Strong),
        WeightedPattern::new(r#"\[string\]\s*\$\w+"#, Strong),
        WeightedPattern::new(r#"\[int\]\s*\$\w+"#, Strong),
        WeightedPattern::new(r#"\[bool\]\s*\$\w+"#, Strong),
        WeightedPattern::new(r#"\[array\]\s*\$\w+"#, Strong),
        WeightedPattern::new(r#"\[hashtable\]\s*\$\w+"#, Strong),
        
        // Common PowerShell cmdlets (8 pts each - Strong)
        WeightedPattern::new(r#"\bWrite-Host\b"#, Strong),
        WeightedPattern::new(r#"\bWrite-Output\b"#, Strong),
        WeightedPattern::new(r#"\bWrite-Error\b"#, Strong),
        WeightedPattern::new(r#"\bWrite-Warning\b"#, Strong),
        WeightedPattern::new(r#"\bWrite-Verbose\b"#, Strong),
        WeightedPattern::new(r#"\bWrite-Debug\b"#, Strong),
        WeightedPattern::new(r#"\bGet-Content\b"#, Strong),
        WeightedPattern::new(r#"\bSet-Content\b"#, Strong),
        WeightedPattern::new(r#"\bGet-ChildItem\b"#, Strong),
        WeightedPattern::new(r#"\bGet-Process\b"#, Strong),
        WeightedPattern::new(r#"\bGet-Service\b"#, Strong),
        WeightedPattern::new(r#"\bTest-Path\b"#, Strong),
        WeightedPattern::new(r#"\bNew-Item\b"#, Strong),
        WeightedPattern::new(r#"\bRemove-Item\b"#, Strong),
        WeightedPattern::new(r#"\bCopy-Item\b"#, Strong),
        WeightedPattern::new(r#"\bMove-Item\b"#, Strong),
        
        // PowerShell control flow (8 pts - Strong)
        WeightedPattern::new(r#"\bforeach\s*\(\s*\$\w+\s+in\s+"#, Strong),
        WeightedPattern::new(r#"\bif\s*\([^)]*-\w+\s+"#, Strong),  // if with PowerShell operator
        WeightedPattern::new(r#"\belseif\s*\("#, Strong),
        WeightedPattern::new(r#"\bswitch\s*\(\s*\$"#, Strong),
        WeightedPattern::new(r#"\btry\s*\{"#, Strong),
        WeightedPattern::new(r#"\bcatch\s*\{"#, Strong),
        WeightedPattern::new(r#"\bfinally\s*\{"#, Strong),
        
        // PowerShell pipeline operators (5 pts - Medium)
        WeightedPattern::new(r#"\|\s*Where-Object"#, Medium),
        WeightedPattern::new(r#"\|\s*Select-Object"#, Medium),
        WeightedPattern::new(r#"\|\s*ForEach-Object"#, Medium),
        WeightedPattern::new(r#"\|\s*Sort-Object"#, Medium),
        WeightedPattern::new(r#"\|\s*Group-Object"#, Medium),
        WeightedPattern::new(r#"\|\s*Measure-Object"#, Medium),
        WeightedPattern::new(r#"\|\s*Out-"#, Medium),
        
        // PowerShell common aliases (5 pts - Medium)
        WeightedPattern::new(r#"\b(ls|dir|cd|pwd|rm|cp|mv|cat|echo|kill|ps|sleep)\s+"#, Medium),
        
        // PowerShell comments (5 pts - Medium)
        WeightedPattern::new(r#"<#[\s\S]*?#>"#, Medium),  // Block comment
        WeightedPattern::new(r##"#[^\n]+$"##, Medium),     // Line comment
        
        // Negative patterns - disqualify PowerShell for other language syntax
        WeightedPattern::new(r#"\bfn\s+\w+\s*\("#, Negative),           // Rust
        WeightedPattern::new(r#"\bdef\s+\w+\s*\("#, Negative),          // Python
        WeightedPattern::new(r#"public\s+class\s+\w+"#, Negative),     // Java
        WeightedPattern::new(r#"\bpackage\s+\w+"#, Negative),          // Go
        WeightedPattern::new(r##"#\[derive\("##, Negative),            // Rust attribute
        WeightedPattern::new(r#"\bimpl\s+\w+\s+for\s+"#, Negative),    // Rust
        WeightedPattern::new(r#"SELECT\s+.+\s+FROM\s+"#, Negative),    // SQL
        WeightedPattern::new(r#"\[package\]"#, Negative),              // TOML (Cargo.toml)
        WeightedPattern::new(r#"\[dependencies\]"#, Negative),         // TOML
    ],
};

pub static TOML: LanguageDefinition = LanguageDefinition {
    name: "toml",
    patterns: &[
        // Unique to TOML (10 pts each)
        WeightedPattern::new(r#"\[\w+\]"#, Unique),
        WeightedPattern::new(r#"\[\[\w+\]\]"#, Unique),
        WeightedPattern::new(r#"\[[\w.]+\]"#, Unique),
        WeightedPattern::new(r#"^\w+\s*=\s*"[^"]*""#, Unique),
        WeightedPattern::new(r#"^\w+\s*=\s*'[^']*'"#, Unique),
        WeightedPattern::new(r#"^\w+\s*=\s*\d+"#, Unique),
        WeightedPattern::new(r#"^\w+\s*=\s*(true|false)"#, Unique),
        WeightedPattern::new(r#"^\w+\s*=\s*\["#, Unique),
        WeightedPattern::new(r#"^\w+\s*=\s*\{"#, Unique),
        WeightedPattern::new(r#"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}"#, Unique),
        WeightedPattern::new(r#"\[package\]"#, Unique),
        WeightedPattern::new(r#"\[dependencies\]"#, Unique),
        WeightedPattern::new(r#"\[dev-dependencies\]"#, Unique),
        WeightedPattern::new(r#"\[build-dependencies\]"#, Unique),
        WeightedPattern::new(r#"\[features\]"#, Unique),
        WeightedPattern::new(r#"\[workspace\]"#, Unique),
        WeightedPattern::new(r#"\[profile\.\w+\]"#, Unique),
        WeightedPattern::new(r#"\[tool\.\w+\]"#, Unique),
        WeightedPattern::new(r#"version\s*=\s*"[^"]*""#, Unique),
        WeightedPattern::new(r#"edition\s*=\s*"\d{4}""#, Unique),
        WeightedPattern::new(r#"name\s*=\s*"[^"]*""#, Unique),
        WeightedPattern::new(r#"path\s*=\s*"[^"]*""#, Unique),
        WeightedPattern::new(r#"workspace\s*=\s*(true|false)"#, Unique),
        
        // Strong indicators (8 pts each)
        WeightedPattern::new(r#"=\s*"[^"]*""#, Strong),
        WeightedPattern::new(r#"=\s*\d+"#, Strong),
        WeightedPattern::new(r#"=\s*(true|false)"#, Strong),
        
        // Medium indicators (5 pts each)
        WeightedPattern::new(r##"#[^\n]+$"##, Medium),
        WeightedPattern::new(r#"\btrue\b"#, Medium),
        WeightedPattern::new(r#"\bfalse\b"#, Medium),
        
        // Negative patterns
        WeightedPattern::new(r#"\bfn\s+"#, Negative),
        WeightedPattern::new(r#"\bdef\s+"#, Negative),
        WeightedPattern::new(r#"\bfunc\s+"#, Negative),
        WeightedPattern::new(r#"function\s+"#, Negative),
        WeightedPattern::new(r#"public\s+class"#, Negative),
        // Shell command patterns - disqualify TOML for shell content
        WeightedPattern::new(r#"\bbrew\s+install"#, Negative),
        WeightedPattern::new(r#"\bapt-get\s+"#, Negative),
        WeightedPattern::new(r#"\bcurl\s+-"#, Negative),
        WeightedPattern::new(r#"\bwget\s+"#, Negative),
        WeightedPattern::new(r#"\bgit\s+(clone|pull|push)"#, Negative),
        WeightedPattern::new(r#"\bnpm\s+(install|run)"#, Negative),
        WeightedPattern::new(r#"\byarn\s+(install|add)"#, Negative),
        WeightedPattern::new(r#"\bcargo\s+(build|run)"#, Negative),
        WeightedPattern::new(r#"\bexport\s+\w+=['\"]"#, Negative),
        WeightedPattern::new(r##"#!/bin/(bash|sh)"##, Negative),
        // PowerShell command patterns - disqualify TOML for PowerShell content
        WeightedPattern::new(r#"\b(irm|iex|iwr)\b"#, Negative),                    // PowerShell aliases
        WeightedPattern::new(r#"\[[A-Z]\w+\]::"#, Negative),                       // .NET type syntax
        WeightedPattern::new(r#"-(eq|ne|like|match|contains)\b"#, Negative),       // PowerShell operators
        WeightedPattern::new(r#"\$(PSScriptRoot|PSVersionTable)\b"#, Negative),    // PowerShell variables
        WeightedPattern::new(r#"\b(Get|Set|Invoke|Write)-[A-Z]\w+"#, Negative),   // PowerShell cmdlets
    ],
};

pub static YAML: LanguageDefinition = LanguageDefinition {
    name: "yaml",
    patterns: &[
        // Unique to YAML (10 pts each)
        WeightedPattern::new(r#"^---\s*$"#, Unique),
        WeightedPattern::new(r#"^\.\.\.\s*$"#, Unique),
        WeightedPattern::new(r#"^\w+:\s*$"#, Unique),
        WeightedPattern::new(r#"^\w+:\s+\|"#, Unique),
        WeightedPattern::new(r#"^\w+:\s+>"#, Unique),
        WeightedPattern::new(r#"^\s+-\s+\w+:\s+"#, Unique),
        WeightedPattern::new(r#"^\s+-\s+["'][^"']*["']"#, Unique),
        WeightedPattern::new(r#"^\s+-\s+\d+"#, Unique),
        WeightedPattern::new(r#"^\s+\w+:\s+"#, Unique),
        WeightedPattern::new(r#"\&\w+"#, Unique),
        WeightedPattern::new(r#"\*\w+"#, Unique),
        WeightedPattern::new(r#"<<:\s*\*\w+"#, Unique),
        WeightedPattern::new(r#"!\w+"#, Unique),
        WeightedPattern::new(r#"!!\w+"#, Unique),
        WeightedPattern::new(r#"\{\{[^}]+\}\}"#, Unique),
        WeightedPattern::new(r#"apiVersion:\s+"#, Unique),
        WeightedPattern::new(r#"kind:\s+"#, Unique),
        WeightedPattern::new(r#"metadata:\s*$"#, Unique),
        WeightedPattern::new(r#"spec:\s*$"#, Unique),
        WeightedPattern::new(r#"name:\s+"#, Unique),
        WeightedPattern::new(r#"image:\s+"#, Unique),
        WeightedPattern::new(r#"version:\s+"#, Unique),
        WeightedPattern::new(r#"services:\s*$"#, Unique),
        WeightedPattern::new(r#"volumes:\s*$"#, Unique),
        WeightedPattern::new(r#"environment:\s*$"#, Unique),
        WeightedPattern::new(r#"ports:\s*$"#, Unique),
        
        // GitHub Actions workflow patterns (10 pts each)
        WeightedPattern::new(r#"on:\s*$"#, Unique),               // Workflow trigger
        WeightedPattern::new(r#"jobs:\s*$"#, Unique),             // Jobs section
        WeightedPattern::new(r#"steps:\s*$"#, Unique),            // Steps array
        WeightedPattern::new(r#"runs-on:\s+"#, Unique),           // Runner specification
        WeightedPattern::new(r#"uses:\s+[^@]+@"#, Unique),        // Action usage (e.g., uses: actions/checkout@v3)
        WeightedPattern::new(r#"with:\s*$"#, Unique),             // Action inputs
        WeightedPattern::new(r#"run:\s+\|"#, Unique),             // Multi-line run command
        WeightedPattern::new(r#"env:\s*$"#, Unique),              // Environment variables section
        
        // GitLab CI patterns (10 pts each)
        WeightedPattern::new(r#"stages:\s*$"#, Unique),           // Pipeline stages
        WeightedPattern::new(r#"script:\s*$"#, Unique),           // Script section
        WeightedPattern::new(r#"before_script:\s*$"#, Unique),    // Before script hook
        WeightedPattern::new(r#"after_script:\s*$"#, Unique),     // After script hook
        WeightedPattern::new(r#"variables:\s*$"#, Unique),        // Variables section
        WeightedPattern::new(r#"only:\s*$"#, Unique),             // Conditional execution
        WeightedPattern::new(r#"except:\s*$"#, Unique),           // Exclusion rules
        
        // Workflow trigger patterns (10 pts each)
        WeightedPattern::new(r#"pull_request:\s*$"#, Unique),     // GitHub PR trigger
        WeightedPattern::new(r#"issue_comment:\s*$"#, Unique),    // GitHub issue comment
        WeightedPattern::new(r#"workflow_dispatch:\s*$"#, Unique), // Manual trigger
        WeightedPattern::new(r#"schedule:\s*$"#, Unique),         // Scheduled workflow
        WeightedPattern::new(r#"types:\s*\["#, Unique),           // Event types filter
        
        // Strong indicators (8 pts each)
        WeightedPattern::new(r#"^\w+:"#, Strong),
        WeightedPattern::new(r#"^\s+-\s+"#, Strong),
        
        // Medium indicators (5 pts each)
        WeightedPattern::new(r##"#[^\n]+$"##, Medium),
        WeightedPattern::new(r#"\btrue\b"#, Medium),
        WeightedPattern::new(r#"\bfalse\b"#, Medium),
        WeightedPattern::new(r#"\bnull\b"#, Medium),
        WeightedPattern::new(r#"\byes\b"#, Medium),
        WeightedPattern::new(r#"\bno\b"#, Medium),
        
        // Negative patterns
        WeightedPattern::new(r#"\bfn\s+"#, Negative),
        WeightedPattern::new(r#"\bdef\s+"#, Negative),
        WeightedPattern::new(r#"\bfunc\s+"#, Negative),
        WeightedPattern::new(r#"function\s+"#, Negative),
        WeightedPattern::new(r#"public\s+class"#, Negative),
        WeightedPattern::new(r#"\{[^}]+:"#, Negative),
        // Shell command patterns - disqualify YAML for shell content
        WeightedPattern::new(r#"\bbrew\s+install"#, Negative),
        WeightedPattern::new(r#"\bapt-get\s+"#, Negative),
        WeightedPattern::new(r#"\bcurl\s+-"#, Negative),
        WeightedPattern::new(r#"\bwget\s+"#, Negative),
        WeightedPattern::new(r#"\bgit\s+(clone|pull|push)"#, Negative),
        WeightedPattern::new(r#"\bnpm\s+(install|run)"#, Negative),
        WeightedPattern::new(r#"\byarn\s+(install|add)"#, Negative),
        WeightedPattern::new(r#"\bcargo\s+(build|run)"#, Negative),
        WeightedPattern::new(r#"\bexport\s+\w+=['\"]"#, Negative),
        WeightedPattern::new(r##"#!/bin/(bash|sh)"##, Negative),
    ],
};

pub static JSON: LanguageDefinition = LanguageDefinition {
    name: "json",
    patterns: &[
        // Unique to JSON (10 pts each)
        WeightedPattern::new(r#"^\s*\{\s*$"#, Unique),
        WeightedPattern::new(r#"^\s*\}\s*$"#, Unique),
        WeightedPattern::new(r#"^\s*\[\s*$"#, Unique),
        WeightedPattern::new(r#"^\s*\]\s*$"#, Unique),
        WeightedPattern::new(r#""[^"]+"\s*:\s*\{"#, Unique),
        WeightedPattern::new(r#""[^"]+"\s*:\s*\["#, Unique),
        WeightedPattern::new(r#""[^"]+"\s*:\s*"[^"]*""#, Unique),
        WeightedPattern::new(r#""[^"]+"\s*:\s*\d+"#, Unique),
        WeightedPattern::new(r#""[^"]+"\s*:\s*(true|false)"#, Unique),
        WeightedPattern::new(r#""[^"]+"\s*:\s*null"#, Unique),
        WeightedPattern::new(r#",\s*$"#, Unique),
        WeightedPattern::new(r#"^\s*"[^"]+"\s*:"#, Unique),
        
        // Strong indicators (8 pts each)
        WeightedPattern::new(r#""[^"]+"\s*:"#, Strong),
        WeightedPattern::new(r#":\s*\["#, Strong),
        WeightedPattern::new(r#":\s*\{"#, Strong),
        
        // Medium indicators (5 pts each)
        WeightedPattern::new(r#"\btrue\b"#, Medium),
        WeightedPattern::new(r#"\bfalse\b"#, Medium),
        WeightedPattern::new(r#"\bnull\b"#, Medium),
        
        // Negative patterns
        WeightedPattern::new(r#"\bfn\s+"#, Negative),
        WeightedPattern::new(r#"\bdef\s+"#, Negative),
        WeightedPattern::new(r#"\bfunc\s+"#, Negative),
        WeightedPattern::new(r#"function\s+"#, Negative),
        WeightedPattern::new(r##"#[^\n]+$"##, Negative),
        WeightedPattern::new(r#"//[^\n]+$"#, Negative),
    ],
};

pub static CSS: LanguageDefinition = LanguageDefinition {
    name: "css",
    patterns: &[
        // Unique to CSS (10 pts each)
        WeightedPattern::new(r##"#[a-zA-Z_][a-zA-Z0-9_-]*\s*\{"##, Unique),
        WeightedPattern::new(r#"\.[a-zA-Z_][a-zA-Z0-9_-]*\s*\{"#, Unique),
        WeightedPattern::new(r#"[a-zA-Z]+\s*\{[^}]*\}"#, Unique),
        WeightedPattern::new(r#"@media\s+"#, Unique),
        WeightedPattern::new(r#"@keyframes\s+"#, Unique),
        WeightedPattern::new(r#"@import\s+"#, Unique),
        WeightedPattern::new(r#"@font-face\s*\{"#, Unique),
        WeightedPattern::new(r#"@charset\s+"#, Unique),
        WeightedPattern::new(r#"@supports\s+"#, Unique),
        WeightedPattern::new(r#"@page\s*\{"#, Unique),
        WeightedPattern::new(r#":\s*hover\s*\{"#, Unique),
        WeightedPattern::new(r#":\s*active\s*\{"#, Unique),
        WeightedPattern::new(r#":\s*focus\s*\{"#, Unique),
        WeightedPattern::new(r#":\s*visited\s*\{"#, Unique),
        WeightedPattern::new(r#":\s*first-child"#, Unique),
        WeightedPattern::new(r#":\s*last-child"#, Unique),
        WeightedPattern::new(r#":\s*nth-child\("#, Unique),
        WeightedPattern::new(r#"::\s*before"#, Unique),
        WeightedPattern::new(r#"::\s*after"#, Unique),
        WeightedPattern::new(r#"::\s*placeholder"#, Unique),
        WeightedPattern::new(r#"background(-color)?:\s*"#, Unique),
        WeightedPattern::new(r#"color:\s*"#, Unique),
        WeightedPattern::new(r#"font(-size|-family|-weight)?:\s*"#, Unique),
        WeightedPattern::new(r#"margin(-top|-right|-bottom|-left)?:\s*"#, Unique),
        WeightedPattern::new(r#"padding(-top|-right|-bottom|-left)?:\s*"#, Unique),
        WeightedPattern::new(r#"border(-radius|-width|-color)?:\s*"#, Unique),
        WeightedPattern::new(r#"display:\s*(block|inline|flex|grid|none)"#, Unique),
        WeightedPattern::new(r#"position:\s*(relative|absolute|fixed|sticky)"#, Unique),
        WeightedPattern::new(r#"flex(-direction|-wrap|-grow|-shrink)?:\s*"#, Unique),
        WeightedPattern::new(r#"grid(-template|-area|-gap)?:\s*"#, Unique),
        WeightedPattern::new(r#"width:\s*"#, Unique),
        WeightedPattern::new(r#"height:\s*"#, Unique),
        WeightedPattern::new(r#"z-index:\s*"#, Unique),
        WeightedPattern::new(r#"opacity:\s*"#, Unique),
        WeightedPattern::new(r#"transform:\s*"#, Unique),
        WeightedPattern::new(r#"transition:\s*"#, Unique),
        WeightedPattern::new(r#"animation:\s*"#, Unique),
        WeightedPattern::new(r#"box-shadow:\s*"#, Unique),
        WeightedPattern::new(r#"text-align:\s*"#, Unique),
        WeightedPattern::new(r#"text-decoration:\s*"#, Unique),
        WeightedPattern::new(r#"line-height:\s*"#, Unique),
        WeightedPattern::new(r#"overflow:\s*"#, Unique),
        WeightedPattern::new(r#"cursor:\s*"#, Unique),
        WeightedPattern::new(r#"visibility:\s*"#, Unique),
        WeightedPattern::new(r##"#[0-9a-fA-F]{3,8}\b"##, Unique),
        WeightedPattern::new(r#"rgb\s*\("#, Unique),
        WeightedPattern::new(r#"rgba\s*\("#, Unique),
        WeightedPattern::new(r#"hsl\s*\("#, Unique),
        WeightedPattern::new(r#"hsla\s*\("#, Unique),
        WeightedPattern::new(r#"var\s*\(--"#, Unique),
        WeightedPattern::new(r#"calc\s*\("#, Unique),
        WeightedPattern::new(r#"\d+px"#, Unique),
        WeightedPattern::new(r#"\d+em"#, Unique),
        WeightedPattern::new(r#"\d+rem"#, Unique),
        WeightedPattern::new(r#"\d+%"#, Unique),
        WeightedPattern::new(r#"\d+vh"#, Unique),
        WeightedPattern::new(r#"\d+vw"#, Unique),
        WeightedPattern::new(r#"!important"#, Unique),
        
        // Strong indicators (8 pts each)
        WeightedPattern::new(r#"\{[^}]*\}"#, Strong),
        WeightedPattern::new(r#"\w+:\s*[^;]+;"#, Strong),
        
        // Medium indicators (5 pts each)
        WeightedPattern::new(r#"/\*[\s\S]*?\*/"#, Medium),
        
        // Negative patterns
        WeightedPattern::new(r#"\bfn\s+"#, Negative),
        WeightedPattern::new(r#"\bdef\s+"#, Negative),
        WeightedPattern::new(r#"\bfunc\s+"#, Negative),
        WeightedPattern::new(r#"function\s+"#, Negative),
        WeightedPattern::new(r#"public\s+class"#, Negative),
        WeightedPattern::new(r#"//[^\n]+$"#, Negative),
        // Shell command patterns - disqualify CSS for shell content
        WeightedPattern::new(r#"\bbrew\s+install"#, Negative),
        WeightedPattern::new(r#"\bapt-get\s+"#, Negative),
        WeightedPattern::new(r#"\bcurl\s+-"#, Negative),
        WeightedPattern::new(r#"\bwget\s+"#, Negative),
        WeightedPattern::new(r#"\bgit\s+(clone|pull|push)"#, Negative),
        WeightedPattern::new(r#"\bnpm\s+(install|run)"#, Negative),
        WeightedPattern::new(r#"\byarn\s+(install|add)"#, Negative),
        WeightedPattern::new(r#"\bcargo\s+(build|run)"#, Negative),
        WeightedPattern::new(r#"\bexport\s+\w+=['\"]"#, Negative),
        WeightedPattern::new(r##"#!/bin/(bash|sh)"##, Negative),
    ],
};

pub static HTML: LanguageDefinition = LanguageDefinition {
    name: "html",
    patterns: &[
        // Unique to HTML (10 pts each)
        WeightedPattern::new(r#"<!DOCTYPE\s+html>"#, Unique),
        WeightedPattern::new(r#"<html[\s>]"#, Unique),
        WeightedPattern::new(r#"</html>"#, Unique),
        WeightedPattern::new(r#"<head[\s>]"#, Unique),
        WeightedPattern::new(r#"</head>"#, Unique),
        WeightedPattern::new(r#"<body[\s>]"#, Unique),
        WeightedPattern::new(r#"</body>"#, Unique),
        WeightedPattern::new(r#"<title>"#, Unique),
        WeightedPattern::new(r#"</title>"#, Unique),
        WeightedPattern::new(r#"<meta\s+"#, Unique),
        WeightedPattern::new(r#"<link\s+"#, Unique),
        WeightedPattern::new(r#"<script[\s>]"#, Unique),
        WeightedPattern::new(r#"</script>"#, Unique),
        WeightedPattern::new(r#"<style[\s>]"#, Unique),
        WeightedPattern::new(r#"</style>"#, Unique),
        WeightedPattern::new(r#"<div[\s>]"#, Unique),
        WeightedPattern::new(r#"</div>"#, Unique),
        WeightedPattern::new(r#"<span[\s>]"#, Unique),
        WeightedPattern::new(r#"</span>"#, Unique),
        WeightedPattern::new(r#"<p[\s>]"#, Unique),
        WeightedPattern::new(r#"</p>"#, Unique),
        WeightedPattern::new(r#"<a\s+href"#, Unique),
        WeightedPattern::new(r#"</a>"#, Unique),
        WeightedPattern::new(r#"<img\s+"#, Unique),
        WeightedPattern::new(r#"<input[\s>]"#, Unique),
        WeightedPattern::new(r#"<button[\s>]"#, Unique),
        WeightedPattern::new(r#"</button>"#, Unique),
        WeightedPattern::new(r#"<form[\s>]"#, Unique),
        WeightedPattern::new(r#"</form>"#, Unique),
        WeightedPattern::new(r#"<table[\s>]"#, Unique),
        WeightedPattern::new(r#"</table>"#, Unique),
        WeightedPattern::new(r#"<tr[\s>]"#, Unique),
        WeightedPattern::new(r#"</tr>"#, Unique),
        WeightedPattern::new(r#"<td[\s>]"#, Unique),
        WeightedPattern::new(r#"</td>"#, Unique),
        WeightedPattern::new(r#"<th[\s>]"#, Unique),
        WeightedPattern::new(r#"</th>"#, Unique),
        WeightedPattern::new(r#"<ul[\s>]"#, Unique),
        WeightedPattern::new(r#"</ul>"#, Unique),
        WeightedPattern::new(r#"<ol[\s>]"#, Unique),
        WeightedPattern::new(r#"</ol>"#, Unique),
        WeightedPattern::new(r#"<li[\s>]"#, Unique),
        WeightedPattern::new(r#"</li>"#, Unique),
        WeightedPattern::new(r#"<h[1-6][\s>]"#, Unique),
        WeightedPattern::new(r#"</h[1-6]>"#, Unique),
        WeightedPattern::new(r#"<nav[\s>]"#, Unique),
        WeightedPattern::new(r#"</nav>"#, Unique),
        WeightedPattern::new(r#"<header[\s>]"#, Unique),
        WeightedPattern::new(r#"</header>"#, Unique),
        WeightedPattern::new(r#"<footer[\s>]"#, Unique),
        WeightedPattern::new(r#"</footer>"#, Unique),
        WeightedPattern::new(r#"<section[\s>]"#, Unique),
        WeightedPattern::new(r#"</section>"#, Unique),
        WeightedPattern::new(r#"<article[\s>]"#, Unique),
        WeightedPattern::new(r#"</article>"#, Unique),
        WeightedPattern::new(r#"<aside[\s>]"#, Unique),
        WeightedPattern::new(r#"</aside>"#, Unique),
        WeightedPattern::new(r#"<main[\s>]"#, Unique),
        WeightedPattern::new(r#"</main>"#, Unique),
        WeightedPattern::new(r#"class="[^"]*""#, Unique),
        WeightedPattern::new(r#"id="[^"]*""#, Unique),
        WeightedPattern::new(r#"style="[^"]*""#, Unique),
        WeightedPattern::new(r#"src="[^"]*""#, Unique),
        WeightedPattern::new(r#"href="[^"]*""#, Unique),
        WeightedPattern::new(r#"alt="[^"]*""#, Unique),
        WeightedPattern::new(r#"type="[^"]*""#, Unique),
        WeightedPattern::new(r#"name="[^"]*""#, Unique),
        WeightedPattern::new(r#"value="[^"]*""#, Unique),
        WeightedPattern::new(r#"data-\w+="[^"]*""#, Unique),
        WeightedPattern::new(r#"onclick="[^"]*""#, Unique),
        WeightedPattern::new(r#"onload="[^"]*""#, Unique),
        
        // Strong indicators (8 pts each)
        WeightedPattern::new(r#"<\w+[\s>]"#, Strong),
        WeightedPattern::new(r#"</\w+>"#, Strong),
        WeightedPattern::new(r#"<\w+\s+\w+="[^"]*""#, Strong),
        WeightedPattern::new(r#"<br\s*/?\s*>"#, Strong),
        WeightedPattern::new(r#"<hr\s*/?\s*>"#, Strong),
        
        // Medium indicators (5 pts each)
        WeightedPattern::new(r#"<!--[\s\S]*?-->"#, Medium),
        WeightedPattern::new(r#"&nbsp;"#, Medium),
        WeightedPattern::new(r#"&lt;"#, Medium),
        WeightedPattern::new(r#"&gt;"#, Medium),
        WeightedPattern::new(r#"&amp;"#, Medium),
        
        // Negative patterns
        WeightedPattern::new(r#"\bfn\s+"#, Negative),
        WeightedPattern::new(r#"\bdef\s+"#, Negative),
        WeightedPattern::new(r#"\bfunc\s+"#, Negative),
        WeightedPattern::new(r#"function\s+\w+\s*\("#, Negative),
        WeightedPattern::new(r#"public\s+class"#, Negative),
    ],
};

pub static XML: LanguageDefinition = LanguageDefinition {
    name: "xml",
    patterns: &[
        // Unique to XML (10 pts each)
        WeightedPattern::new(r#"<\?xml\s+version"#, Unique),
        WeightedPattern::new(r#"<\?xml-stylesheet"#, Unique),
        WeightedPattern::new(r#"<!DOCTYPE\s+\w+"#, Unique),
        WeightedPattern::new(r#"<!\[CDATA\["#, Unique),
        WeightedPattern::new(r#"\]\]>"#, Unique),
        WeightedPattern::new(r#"xmlns:\w+="#, Unique),
        WeightedPattern::new(r#"xmlns="#, Unique),
        WeightedPattern::new(r#"xsi:schemaLocation"#, Unique),
        WeightedPattern::new(r#"<\w+:\w+[\s>]"#, Unique),
        WeightedPattern::new(r#"</\w+:\w+>"#, Unique),
        WeightedPattern::new(r#"<\w+\s+\w+:\w+="[^"]*""#, Unique),
        WeightedPattern::new(r#"<!ENTITY\s+"#, Unique),
        WeightedPattern::new(r#"<!ELEMENT\s+"#, Unique),
        WeightedPattern::new(r#"<!ATTLIST\s+"#, Unique),
        WeightedPattern::new(r#"&\w+;"#, Unique),
        
        // Strong indicators (8 pts each)
        WeightedPattern::new(r#"<\w+[\s>]"#, Strong),
        WeightedPattern::new(r#"</\w+>"#, Strong),
        WeightedPattern::new(r#"<\w+\s+\w+="[^"]*""#, Strong),
        WeightedPattern::new(r#"/>"#, Strong),
        
        // Medium indicators (5 pts each)
        WeightedPattern::new(r#"<!--[\s\S]*?-->"#, Medium),
        
        // Negative patterns
        WeightedPattern::new(r#"<!DOCTYPE\s+html>"#, Negative),
        WeightedPattern::new(r#"<html[\s>]"#, Negative),
        WeightedPattern::new(r#"\bfn\s+"#, Negative),
        WeightedPattern::new(r#"\bdef\s+"#, Negative),
        WeightedPattern::new(r#"\bfunc\s+"#, Negative),
        WeightedPattern::new(r#"function\s+"#, Negative),
    ],
};

/// All language definitions in priority order (most specific first)
pub static ALL_LANGUAGES: &[&LanguageDefinition] = &[
    &RUST,
    &TYPESCRIPT,
    &GO,
    &JAVA,
    &CPP,
    &C_LANG,
    &PYTHON,
    &RUBY,
    &PHP,
    &SHELL,
    &POWERSHELL,
    &SQL,
    &JAVASCRIPT,
    &TOML,
    &YAML,
    &JSON,
    &CSS,
    &HTML,
    &XML,
];
