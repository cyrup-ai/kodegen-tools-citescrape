//! JavaScript evaluation scripts
//!
//! This module contains the JavaScript code used to extract various types
//! of data from web pages.

/// JavaScript script to extract page metadata
pub const METADATA_SCRIPT: &str = r#"
    (() => {
        const meta = {};
        document.querySelectorAll('meta').forEach(tag => {
            const name = tag.getAttribute('name') || tag.getAttribute('property');
            if (name) {
                meta[name] = tag.getAttribute('content');
            }
        });

        return {
            description: meta['description'] || meta['og:description'] || null,
            keywords: meta['keywords'] ? meta['keywords'].split(',').map(k => k.trim()) : [],
            author: meta['author'] || meta['og:author'] || null,
            published_date: meta['article:published_time'] || meta['publishedDate'] || null,
            modified_date: meta['article:modified_time'] || meta['modifiedDate'] || null,
            language: document.documentElement.lang || null,
            canonical_url: document.querySelector('link[rel="canonical"]')?.href || null,
            robots: meta['robots'] || null,
            viewport: meta['viewport'] || null,
            headers: {}
        };
    })()
"#;

/// JavaScript script to extract page resources
pub const RESOURCES_SCRIPT: &str = r#"
    (() => {
        const scripts = Array.from(document.getElementsByTagName('script'))
            .filter(script => script.src)
            .map(script => ({
                url: script.src,
                inline: false,
                async_load: script.async,
                defer: script.defer,
                content_hash: null
            }));

        const stylesheets = Array.from(document.getElementsByTagName('link'))
            .filter(link => link.rel === 'stylesheet' && link.href)
            .map(style => ({
                url: style.href,
                inline: false,
                media: style.media || null,
                content_hash: null
            }));

        const images = Array.from(document.getElementsByTagName('img'))
            .filter(img => img.src)
            .map(img => ({
                url: img.src,
                alt: img.alt || null,
                dimensions: img.width && img.height ? [img.width, img.height] : null,
                size_bytes: null,
                format: img.src.split('.').pop()?.split('?')[0] || null
            }));

        const media = Array.from(document.querySelectorAll('video, audio'))
            .map(media => {
                const source = media.querySelector('source');
                const url = media.src || source?.src;
                return url ? {
                    url: url,
                    media_type: media.tagName.toLowerCase(),
                    format: url.split('.').pop()?.split('?')[0] || null,
                    duration: media.duration || null,
                    size_bytes: null
                } : null;
            })
            .filter(media => media !== null);

        const fonts = Array.from(document.querySelectorAll('link[rel="preload"][as="font"], link[rel="font"]'))
            .filter(font => font.href)
            .map(font => ({
                url: font.href,
                format: font.getAttribute('type') || null,
                family: font.getAttribute('font-family') || 'unknown',
                weight: font.getAttribute('font-weight') ? parseInt(font.getAttribute('font-weight')) : null,
                style: font.getAttribute('font-style') || null
            }));

        return {
            scripts,
            stylesheets,
            images,
            media,
            fonts
        };
    })()
"#;

/// JavaScript script to extract timing information
pub const TIMING_SCRIPT: &str = r"
    (() => {
        const timing = performance.timing || {};
        const nav = performance.getEntriesByType('navigation')[0] || {};
        
        return {
            navigation_start: timing.navigationStart || nav.startTime || 0,
            response_end: timing.responseEnd || nav.responseEnd || 0,
            dom_complete: timing.domComplete || nav.domComplete || 0,
            load_complete: timing.loadEventEnd || nav.loadEventEnd || 0,
            total_duration: (timing.loadEventEnd || nav.loadEventEnd || 0) - 
                        (timing.navigationStart || nav.startTime || 0)
        };
    })()
";

/// JavaScript script to extract security information
pub const SECURITY_SCRIPT: &str = r#"
    (() => {
        const url = new URL(window.location.href);
        const cert = window.performance.getEntriesByType('resource')
            .find(entry => entry.name === window.location.href);

        return {
            https: url.protocol === 'https:',
            hsts: document.location.protocol === 'https:' && 
                document.querySelector('meta[http-equiv="Strict-Transport-Security"]') !== null,
            csp: document.querySelector('meta[http-equiv="Content-Security-Policy"]')?.content || '',
            x_frame_options: document.querySelector('meta[http-equiv="X-Frame-Options"]')?.content || '',
            permissions_policy: document.querySelector('meta[http-equiv="Permissions-Policy"]')?.content || ''
        };
    })()
"#;

/// JavaScript script to extract interactive elements with unique data attributes
pub const INTERACTIVE_ELEMENTS_SCRIPT: &str = r#"
    (() => {
        // Comprehensive interactive elements query
        const selector = [
            // Standard form controls
            'button', 'input', 'select', 'textarea',
            
            // Links
            'a[href]',
            
            // Native interactive HTML elements
            'details', 'summary', 'dialog', 'menu',
            
            // Labels (interactive when associated with input)
            'label[for]',
            
            // Event handlers (any element with these is interactive)
            '[onclick]', '[onsubmit]', '[onchange]', '[ondblclick]',
            
            // Interactive ARIA roles
            '[role="button"]',
            '[role="checkbox"]',
            '[role="radio"]',
            '[role="switch"]',
            '[role="tab"]',
            '[role="slider"]',
            '[role="spinbutton"]',
            '[role="menuitem"]',
            '[role="menuitemcheckbox"]',
            '[role="menuitemradio"]',
            '[role="option"]',
            '[role="link"]',
            '[role="searchbox"]',
            '[role="textbox"]',
            '[role="combobox"]',
            '[role="gridcell"]',
            '[role="treeitem"]',
            
            // Other interactive attributes
            '[contenteditable="true"]',
            '[draggable="true"]',
            '[tabindex]'
        ].join(', ');
        
        const elements = document.querySelectorAll(selector);
        
        return Array.from(elements).map((el) => {
            return {
                element_type: el.tagName.toLowerCase(),
                text: el.textContent?.trim() || null,
                url: el.href || null,
                attributes: Object.fromEntries(
                    Array.from(el.attributes).map(attr => [attr.name, attr.value])
                )
            };
        });
    })()
"#;

/// JavaScript script to extract links
pub const LINKS_SCRIPT: &str = r"
    (() => {
        const currentUrl = new URL(window.location.href);
        const links = Array.from(document.querySelectorAll('a[href]'))
            .map(link => {
                const href = link.getAttribute('href');
                if (!href) return null;
                
                try {
                    // Handle relative URLs
                    const absoluteUrl = new URL(href, window.location.href);
                    
                    // Only return http/https links
                    if (!['http:', 'https:'].includes(absoluteUrl.protocol)) {
                        return null;
                    }
                    
                    return {
                        url: absoluteUrl.href,
                        text: link.textContent?.trim() || '',
                        title: link.getAttribute('title') || '',
                        rel: link.getAttribute('rel') || '',
                        is_external: absoluteUrl.host !== currentUrl.host,
                        path: absoluteUrl.pathname
                    };
                } catch (e) {
                    return null;
                }
            })
            .filter(link => link !== null);
            
        // Remove duplicates by URL
        const uniqueLinks = [];
        const seenUrls = new Set();
        
        for (const link of links) {
            if (!seenUrls.has(link.url)) {
                seenUrls.add(link.url);
                uniqueLinks.push(link);
            }
        }
        
        return uniqueLinks;
    })()
";

/// JavaScript script to extract document headings with ordinal hierarchy
///
/// This script:
/// 1. Queries all H1-H6 elements in document order
/// 2. Tracks ordinal counters [h1_count, h2_count, h3_count, h4_count, h5_count, h6_count]
/// 3. For each heading:
///    - Increments counter at its level
///    - Resets all deeper level counters (e.g., new H2 resets H3-H6 counters)
///    - Builds ordinal array from non-zero counters up to current level
/// 4. Returns array of heading objects with level, text, id, and ordinal
///
/// # Ordinal Algorithm
///
/// The ordinal tracking algorithm maintains document hierarchy:
/// - Each heading level (1-6) has a counter
/// - When a heading is encountered:
///   1. Increment its level counter
///   2. Reset all deeper counters (levels > current)
///   3. Build ordinal from counters[0..level] excluding zeros
///
/// Example progression:
/// ```text
/// H1 "Introduction"     → counters = [1,0,0,0,0,0] → ordinal = [1]
/// H2 "Overview"         → counters = [1,1,0,0,0,0] → ordinal = [1,1]
/// H2 "Details"          → counters = [1,2,0,0,0,0] → ordinal = [1,2]
/// H3 "Subsection"       → counters = [1,2,1,0,0,0] → ordinal = [1,2,1]
/// H1 "Next Chapter"     → counters = [2,0,0,0,0,0] → ordinal = [2]
/// ```
pub const HEADINGS_SCRIPT: &str = r#"
    (() => {
        const headings = [];
        const ordinalCounters = [0, 0, 0, 0, 0, 0]; // counters for h1-h6
        
        document.querySelectorAll('h1, h2, h3, h4, h5, h6').forEach(heading => {
            const level = parseInt(heading.tagName.substring(1));
            
            // Increment counter for this level
            ordinalCounters[level - 1]++;
            
            // Reset counters for deeper levels
            // Example: When we hit H2, reset H3, H4, H5, H6 counters
            for (let i = level; i < 6; i++) {
                ordinalCounters[i] = 0;
            }
            
            // Build ordinal path from non-zero counters up to current level
            // Example: [1, 2, 1, 0, 0, 0] → [1, 2, 1]
            const ordinal = ordinalCounters.slice(0, level).filter(n => n > 0);
            
            headings.push({
                level: level,
                text: heading.textContent.trim(),
                id: heading.id || null,
                ordinal: ordinal
            });
        });
        
        return headings;
    })()
"#;
