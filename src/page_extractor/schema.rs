use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResourceType {
    Stylesheet,
    Script,
    Image,
    Font,
    Media,
    Document,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InteractiveElementType {
    Form,
    Button,
    Link,
    Input,
    Clickable,
    Navigation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveElement {
    pub element_type: String,
    pub selector: String,
    pub text: Option<String>,
    pub url: Option<String>,
    pub attributes: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InteractiveElements {
    pub forms: Vec<FormElement>,
    pub buttons: Vec<ButtonElement>,
    pub links: Vec<LinkElement>,
    pub inputs: Vec<InputElement>,
    pub clickable: Vec<ClickableElement>,
    pub navigation: Vec<NavigationElement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormElement {
    pub id: Option<String>,
    pub action: Option<String>,
    pub method: Option<String>,
    pub inputs: Vec<InputElement>,
    pub buttons: Vec<ButtonElement>,
    pub selector: String,
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputElement {
    pub id: Option<String>,
    pub name: Option<String>,
    pub input_type: String,
    pub value: Option<String>,
    pub placeholder: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub disabled: bool,
    pub selector: String,
    pub validation: Option<InputValidation>,
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputValidation {
    pub pattern: Option<String>,
    pub min_length: Option<u32>,
    pub max_length: Option<u32>,
    pub min: Option<String>,
    pub max: Option<String>,
    pub step: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ButtonElement {
    pub id: Option<String>,
    pub text: Option<String>,
    pub button_type: Option<String>,
    pub selector: String,
    #[serde(default)]
    pub disabled: bool,
    pub form_id: Option<String>,
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkElement {
    pub href: String,
    pub text: Option<String>,
    pub title: Option<String>,
    pub target: Option<String>,
    pub rel: Option<String>,
    pub selector: String,
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClickableElement {
    pub selector: String,
    pub text: Option<String>,
    pub role: Option<String>,
    pub aria_label: Option<String>,
    pub event_handlers: Vec<String>,
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigationElement {
    pub selector: String,
    pub text: Option<String>,
    pub url: Option<String>,
    pub nav_type: String, // "menu", "breadcrumb", "pagination", etc.
    pub level: Option<u32>,
    pub parent_selector: Option<String>,
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageData {
    pub url: String,
    pub title: String,
    pub content: String,
    pub metadata: PageMetadata,
    pub interactive_elements: InteractiveElements,
    pub links: Vec<CrawlLink>,
    pub resources: ResourceInfo,
    pub timing: TimingInfo,
    pub security: SecurityInfo,
    pub crawled_at: chrono::DateTime<chrono::Utc>,
}

/// Document heading element with hierarchical position tracking
///
/// Represents a heading (H1-H6) with its ordinal position in the document's
/// heading hierarchy. The ordinal array tracks the hierarchical path, where
/// each number represents the count at that heading level.
///
/// # Ordinal Hierarchy Examples
///
/// - `[1]` - First H1
/// - `[1, 1]` - First H2 under first H1
/// - `[1, 2]` - Second H2 under first H1
/// - `[1, 2, 1]` - First H3 under second H2 under first H1
/// - `[2]` - Second H1 (resets all deeper counters)
/// - `[2, 1]` - First H2 under second H1
///
/// # Fields
///
/// * `level` - Heading level (1-6 for h1-h6)
/// * `text` - Heading text content (trimmed)
/// * `id` - HTML id attribute if present (for anchor linking)
/// * `ordinal` - Hierarchical position as array (e.g., [1, 2, 1])
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadingElement {
    /// Heading level: 1 for h1, 2 for h2, ..., 6 for h6
    pub level: u8,
    
    /// Heading text content (trimmed of whitespace)
    pub text: String,
    
    /// HTML id attribute if present (for anchor links)
    pub id: Option<String>,
    
    /// Hierarchical position array
    /// Example: [1, 2, 1] means "1st H1 → 2nd H2 → 1st H3"
    pub ordinal: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PageMetadata {
    pub description: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub author: Option<String>,
    pub published_date: Option<String>,
    pub modified_date: Option<String>,
    pub language: Option<String>,
    pub canonical_url: Option<String>,
    pub robots: Option<String>,
    pub viewport: Option<String>,
    pub headers: HashMap<String, String>,
    
    /// Document headings (H1-H6) with hierarchical ordinal tracking
    /// Extracted from DOM in document order with position metadata
    #[serde(default)]
    pub headings: Vec<HeadingElement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceInfo {
    pub scripts: Vec<ScriptResource>,
    pub stylesheets: Vec<StyleResource>,
    pub images: Vec<ImageResource>,
    pub media: Vec<MediaResource>,
    pub fonts: Vec<FontResource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptResource {
    pub url: Option<String>,
    #[serde(default)]
    pub inline: bool,
    #[serde(default)]
    pub async_load: bool,
    #[serde(default)]
    pub defer: bool,
    pub content_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleResource {
    pub url: Option<String>,
    #[serde(default)]
    pub inline: bool,
    pub media: Option<String>,
    pub content_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageResource {
    pub url: String,
    pub alt: Option<String>,
    pub dimensions: Option<(u32, u32)>,
    pub size_bytes: Option<u64>,
    pub format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaResource {
    pub url: String,
    pub media_type: String, // "video", "audio"
    pub format: Option<String>,
    pub duration: Option<f64>,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontResource {
    pub url: Option<String>,
    pub format: Option<String>,
    pub family: String,
    pub weight: Option<u32>,
    pub style: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimingInfo {
    pub navigation_start: u64,
    pub dom_complete: u64,
    pub load_complete: u64,
    pub first_paint: Option<u64>,
    pub first_contentful_paint: Option<u64>,
    pub largest_contentful_paint: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityInfo {
    pub https: bool,
    pub hsts: bool,
    pub csp: Option<String>,
    pub x_frame_options: Option<String>,
    pub permissions_policy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewportDims {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlLink {
    pub url: String,
    pub text: String,
    pub title: String,
    pub rel: String,
    pub is_external: bool,
    pub path: String,
}

impl Default for PageData {
    fn default() -> Self {
        Self {
            url: String::new(),
            title: String::new(),
            content: String::new(),
            metadata: Default::default(),
            interactive_elements: InteractiveElements {
                forms: Vec::new(),
                buttons: Vec::new(),
                links: Vec::new(),
                inputs: Vec::new(),
                clickable: Vec::new(),
                navigation: Vec::new(),
            },
            links: Vec::new(),
            resources: Default::default(),
            timing: Default::default(),
            security: Default::default(),
            crawled_at: chrono::Utc::now(),
        }
    }
}
