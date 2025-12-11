use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::str::FromStr;
use url::Url;

use std::sync::Arc;

/// An immutable, cheaply-cloneable URL wrapper.
/// 
/// `ImUrl` provides an efficient way to work with URLs by sharing the parsed
/// `Url` instance via `Arc` while maintaining immutability. All mutation methods
/// return a new `ImUrl` instance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImUrl {
    url_str: Cow<'static, str>,
    url: Arc<Url>,
}

impl ImUrl {
    pub fn parse(input: &str) -> Result<Self> {
        let parsed_url = Url::parse(input).context("Failed to parse URL")?;
        let url_str = Cow::Owned(parsed_url.as_str().to_string());
        let url = Arc::new(parsed_url);
        Ok(Self { url_str, url })
    }

    pub fn as_str(&self) -> &str {
        &self.url_str
    }

    pub fn scheme(&self) -> &str {
        self.url.scheme()
    }

    pub fn host(&self) -> Option<&str> {
        self.url.host_str()
    }

    pub fn port(&self) -> Option<u16> {
        self.url.port()
    }

    pub fn path(&self) -> &str {
        self.url.path()
    }

    pub fn query(&self) -> Option<&str> {
        self.url.query()
    }

    pub fn fragment(&self) -> Option<&str> {
        self.url.fragment()
    }

    /// Returns a normalized URL with the fragment removed.
    ///
    /// This is essential for URL deduplication in web crawling, where
    /// fragment anchors (#section) represent the same HTTP resource.
    ///
    /// # Examples
    ///
    /// ```
    /// let url = ImUrl::parse("https://example.com/page#section1")?;
    /// let normalized = url.without_fragment()?;
    /// assert_eq!(normalized.as_str(), "https://example.com/page");
    /// ```
    pub fn without_fragment(&self) -> Result<Self> {
        let mut url = (*self.url).clone();
        url.set_fragment(None);
        Self::parse(url.as_str())
    }

    pub fn with_path(&self, path: &str) -> Result<Self> {
        let mut url = (*self.url).clone();
        url.set_path(path);
        Self::parse(url.as_str())
    }

    pub fn with_query(&self, query: &str) -> Result<Self> {
        let mut url = (*self.url).clone();
        url.set_query(Some(query));
        Self::parse(url.as_str())
    }

    pub fn with_fragment(&self, fragment: &str) -> Result<Self> {
        let mut url = (*self.url).clone();
        url.set_fragment(Some(fragment));
        Self::parse(url.as_str())
    }

    pub fn with_scheme(&self, scheme: &str) -> Result<Self> {
        let mut url = (*self.url).clone();
        url.set_scheme(scheme)
            .map_err(|_| anyhow::anyhow!("Failed to set scheme"))?;
        Self::parse(url.as_str())
    }

    pub fn with_username(&self, username: &str) -> Result<Self> {
        let mut url = (*self.url).clone();
        url.set_username(username)
            .map_err(|_| anyhow::anyhow!("Failed to set username"))?;
        Self::parse(url.as_str())
    }

    pub fn with_password(&self, password: Option<&str>) -> Result<Self> {
        let mut url = (*self.url).clone();
        url.set_password(password)
            .map_err(|_| anyhow::anyhow!("Failed to set password"))?;
        Self::parse(url.as_str())
    }

    pub fn with_host(&self, host: &str) -> Result<Self> {
        let mut url = (*self.url).clone();
        url.set_host(Some(host)).context("Failed to set host")?;
        Self::parse(url.as_str())
    }

    pub fn with_port(&self, port: u16) -> Result<Self> {
        let mut url = (*self.url).clone();
        url.set_port(Some(port))
            .map_err(|_| anyhow::anyhow!("Failed to set port"))?;
        Self::parse(url.as_str())
    }

    pub fn with_path_segments<I, S>(&self, segments: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut url = (*self.url).clone();
        url.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("Cannot be a base URL"))?
            .clear()
            .extend(segments);
        Self::parse(url.as_str())
    }
}

impl fmt::Display for ImUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.url_str)
    }
}

impl Hash for ImUrl {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.url_str.hash(state);
    }
}

impl FromStr for ImUrl {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

impl TryFrom<String> for ImUrl {
    type Error = anyhow::Error;

    fn try_from(s: String) -> Result<Self> {
        Self::parse(&s)
    }
}

impl TryFrom<&String> for ImUrl {
    type Error = anyhow::Error;

    fn try_from(s: &String) -> Result<Self> {
        Self::parse(s)
    }
}

impl TryFrom<&str> for ImUrl {
    type Error = anyhow::Error;

    fn try_from(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

impl AsRef<str> for ImUrl {
    fn as_ref(&self) -> &str {
        &self.url_str
    }
}

impl AsRef<Url> for ImUrl {
    fn as_ref(&self) -> &Url {
        &self.url
    }
}

impl Deref for ImUrl {
    type Target = Url;

    fn deref(&self) -> &Self::Target {
        &self.url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        let url = ImUrl::parse("https://example.com/path?query=value#fragment").unwrap();
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host(), Some("example.com"));
        assert_eq!(url.path(), "/path");
        assert_eq!(url.query(), Some("query=value"));
        assert_eq!(url.fragment(), Some("fragment"));
    }

    #[test]
    fn test_as_str() {
        let url = ImUrl::parse("https://example.com").unwrap();
        assert_eq!(url.as_str(), "https://example.com/");
    }

    #[test]
    fn test_with_path() {
        let url = ImUrl::parse("https://example.com").unwrap();
        let new_url = url.with_path("/new/path").unwrap();
        assert_eq!(new_url.path(), "/new/path");
    }

    #[test]
    fn test_with_query() {
        let url = ImUrl::parse("https://example.com").unwrap();
        let new_url = url.with_query("foo=bar").unwrap();
        assert_eq!(new_url.query(), Some("foo=bar"));
    }

    #[test]
    fn test_clone_is_cheap() {
        let url1 = ImUrl::parse("https://example.com").unwrap();
        let url2 = url1.clone();
        assert_eq!(url1, url2);
        assert!(Arc::ptr_eq(&url1.url, &url2.url));
    }

    #[test]
    fn test_hash_consistency() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let url1 = ImUrl::parse("https://example.com").unwrap();
        let url2 = ImUrl::parse("https://example.com").unwrap();

        let mut hasher1 = DefaultHasher::new();
        url1.hash(&mut hasher1);
        let hash1 = hasher1.finish();

        let mut hasher2 = DefaultHasher::new();
        url2.hash(&mut hasher2);
        let hash2 = hasher2.finish();

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_from_str() {
        let url: ImUrl = "https://example.com".parse().unwrap();
        assert_eq!(url.as_str(), "https://example.com/");
    }

    #[test]
    fn test_try_from_string() {
        let s = String::from("https://example.com");
        let url = ImUrl::try_from(s).unwrap();
        assert_eq!(url.as_str(), "https://example.com/");
    }

    #[test]
    fn test_as_ref_str() {
        let url = ImUrl::parse("https://example.com").unwrap();
        let s: &str = url.as_ref();
        assert_eq!(s, "https://example.com/");
    }

    #[test]
    fn test_deref() {
        let url = ImUrl::parse("https://example.com/path").unwrap();
        // Should be able to call Url methods directly
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.path(), "/path");
    }

    #[test]
    fn test_serde() {
        let url = ImUrl::parse("https://example.com").unwrap();
        let json = serde_json::to_string(&url).unwrap();
        let deserialized: ImUrl = serde_json::from_str(&json).unwrap();
        assert_eq!(url, deserialized);
    }
}
