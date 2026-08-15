//! Native `GlyphWiki` search, dependency resolution, and image transport.
//!
//! `GlyphWiki`'s editor endpoints return form-encoded plain text rather than
//! JSON. A native client can call the official HTTPS host directly; the
//! browser-only CORS proxy used by web editors is intentionally unnecessary.

use std::collections::{BTreeSet, HashSet};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(test)]
use gpui_vue::http::BodyInner;
use gpui_vue::http::{AsyncBody, HttpClient, HttpResult, Request, Response, Url, anyhow, http};

use super::model::{ComponentDefinition, ComponentLibrary, parse_kage};

/// Official `GlyphWiki` API origin used by the native editor.
const GLYPHWIKI_ORIGIN: &str = "https://glyphwiki.org";
/// A complete search response is bounded before it reaches the UI.
const MAX_SEARCH_BODY_BYTES: u64 = 1024 * 1024;
/// Individual KAGE sources should remain comfortably below this limit.
const MAX_SOURCE_BODY_BYTES: u64 = 256 * 1024;
/// Remote thumbnails are small, but the generic GPUI loader still has a cap.
const MAX_IMAGE_BODY_BYTES: u64 = 8 * 1024 * 1024;
/// Prevents a malicious or broken component graph from growing without bound.
const MAX_COMPONENTS_PER_LOAD: usize = 128;
/// Prevents excessive recursion even when every component name is unique.
const MAX_COMPONENT_DEPTH: usize = 32;
/// The initial component browser never asks the service for an unbounded list.
const MAX_RANDOM_NAMES: usize = 50;
/// A random supplementary-plane prefix normally returns dozens of names.
const MAX_RANDOM_SEARCH_REQUESTS: usize = 4;
/// Assigned CJK blocks expressed as four-hex-digit `search4ge.cgi` prefixes.
///
/// Every prefix covers sixteen code points. Keeping the ranges explicit avoids
/// wasting requests on the large unassigned gaps between CJK extensions.
const RANDOM_CJK_PREFIX_RANGES: &[(u32, u32)] = &[
    (0x2000, 0x2a6d), // CJK Unified Ideographs Extension B
    (0x2a70, 0x2b73), // Extension C
    (0x2b74, 0x2b81), // Extension D
    (0x2b82, 0x2cea), // Extension E
    (0x2ceb, 0x2ebe), // Extension F
    (0x2ebf, 0x2ee5), // Extension I
    (0x2f80, 0x2fa1), // CJK Compatibility Ideographs Supplement
    (0x3000, 0x3134), // Extension G
    (0x3135, 0x323a), // Extension H
];
/// User agent shared by API and thumbnail requests.
const USER_AGENT: &str = "gpui-vue-kage-editor/0.1 (+https://github.com/wcshds/gpui-vue)";

/// Server-level outcomes returned by `search4ge.cgi`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SearchResponse {
    /// Ordered `GlyphWiki` names exactly as returned by the service.
    Matches(Vec<String>),
    /// `GlyphWiki` asks for a more specific query.
    TooShort,
    /// No glyph names match the query.
    NoData,
}

/// A network, protocol, or component-graph failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GlyphWikiError {
    /// The HTTP request could not be completed.
    Network(String),
    /// The server returned a non-success status.
    HttpStatus(u16),
    /// The endpoint response omitted its `data` field.
    MissingData,
    /// The requested component does not exist.
    NotFound(String),
    /// A remote component contains structurally invalid KAGE data.
    InvalidSource { name: String, message: String },
    /// The component graph contains a cycle.
    DependencyCycle(String),
    /// The component graph exceeded a defensive size/depth limit.
    DependencyLimit,
    /// Bounded random searches did not yield the requested number of names.
    RandomPoolExhausted { requested: usize, found: usize },
}

impl Display for GlyphWikiError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network(message) => write!(formatter, "GlyphWiki request failed: {message}"),
            Self::HttpStatus(status) => {
                write!(formatter, "GlyphWiki returned HTTP status {status}")
            }
            Self::MissingData => formatter.write_str("GlyphWiki response has no data field"),
            Self::NotFound(name) => write!(formatter, "GlyphWiki component {name:?} was not found"),
            Self::InvalidSource { name, message } => {
                write!(
                    formatter,
                    "GlyphWiki component {name:?} is invalid: {message}"
                )
            }
            Self::DependencyCycle(name) => {
                write!(formatter, "GlyphWiki component cycle detected at {name:?}")
            }
            Self::DependencyLimit => {
                formatter.write_str("GlyphWiki component dependency limit exceeded")
            }
            Self::RandomPoolExhausted { requested, found } => write!(
                formatter,
                "GlyphWiki random search returned only {found} of {requested} requested names"
            ),
        }
    }
}

impl Error for GlyphWikiError {}

/// Cloneable client for `GlyphWiki`'s public editor endpoints.
#[derive(Clone)]
pub struct GlyphWikiClient {
    /// Reused connection pool and TLS configuration.
    agent: ureq::Agent,
    /// Configurable only for isolated unit tests.
    origin: String,
}

impl Default for GlyphWikiClient {
    fn default() -> Self {
        Self::new()
    }
}

impl GlyphWikiClient {
    /// Creates a client that talks directly to the official HTTPS origin.
    #[must_use]
    pub fn new() -> Self {
        Self {
            agent: shared_agent().clone(),
            origin: GLYPHWIKI_ORIGIN.to_owned(),
        }
    }

    /// Searches `GlyphWiki` while preserving server result order.
    ///
    /// # Errors
    ///
    /// Returns [`GlyphWikiError`] for transport, status, or malformed-body
    /// failures. `tooshort` and `nodata` are successful server outcomes.
    pub fn search(&self, query: &str) -> Result<SearchResponse, GlyphWikiError> {
        let body = self.get_text("/search4ge.cgi", "query", query, MAX_SEARCH_BODY_BYTES)?;
        parse_search_body(&body)
    }

    /// Returns up to fifty shuffled, unique names from live `GlyphWiki` data.
    ///
    /// `GlyphWiki` has no public random/batch endpoint. This uses a small,
    /// bounded number of official `search4ge.cgi` requests for randomly chosen
    /// prefixes inside assigned CJK extension ranges. Each matching name came
    /// from `GlyphWiki`'s current search index; source remains loaded lazily when
    /// the user chooses a component.
    ///
    /// # Errors
    ///
    /// Returns the last request failure, or [`GlyphWikiError::RandomPoolExhausted`]
    /// if four successful searches still contain too few unique names.
    pub fn random_names(&self, limit: usize) -> Result<Vec<String>, GlyphWikiError> {
        random_names_with(limit, random_seed(), |query| self.search(query))
    }

    /// Loads one component and every missing type-99 dependency atomically.
    ///
    /// Definitions already present in `known` are reused without a request.
    /// The returned batch can therefore be committed to a cloned library in
    /// one step without exposing partially resolved geometry to the renderer.
    ///
    /// # Errors
    ///
    /// Returns [`GlyphWikiError`] when any source is missing, invalid, cyclic,
    /// or exceeds the defensive dependency limits.
    pub fn load_component_tree(
        &self,
        root: &str,
        known: &ComponentLibrary,
    ) -> Result<Vec<ComponentDefinition>, GlyphWikiError> {
        resolve_component_tree_with(root, known, |name| self.source(name))
    }

    /// Fetches one raw KAGE source by `GlyphWiki` name.
    fn source(&self, name: &str) -> Result<String, GlyphWikiError> {
        let body = self.get_text("/get_source.cgi", "name", name, MAX_SOURCE_BODY_BYTES)?;
        let data = parse_data_field(&body)?;
        if data == "nodata" || data.is_empty() {
            return Err(GlyphWikiError::NotFound(name.to_owned()));
        }
        Ok(data)
    }

    /// Performs one bounded, form-style GET request.
    fn get_text(
        &self,
        path: &str,
        key: &str,
        value: &str,
        limit: u64,
    ) -> Result<String, GlyphWikiError> {
        let url = format!("{}{path}", self.origin);
        let mut response = self
            .agent
            .get(url)
            .query(key, value)
            .call()
            .map_err(|error| GlyphWikiError::Network(error.to_string()))?;
        if !response.status().is_success() {
            return Err(GlyphWikiError::HttpStatus(response.status().as_u16()));
        }
        response
            .body_mut()
            .with_config()
            .limit(limit)
            .lossy_utf8(false)
            .read_to_string()
            .map_err(|error| GlyphWikiError::Network(error.to_string()))
    }
}

/// Returns a safely encoded `GlyphWiki` 50-pixel thumbnail URL.
#[must_use]
pub fn thumbnail_url(name: &str) -> String {
    let mut url = url::Url::parse(GLYPHWIKI_ORIGIN).expect("static GlyphWiki origin is valid");
    url.path_segments_mut()
        .expect("GlyphWiki origin supports path segments")
        .push("glyph")
        .push(&format!("{name}.50px.png"));
    url.into()
}

/// GPUI HTTP adapter used for `GlyphWiki`'s lazy 50-pixel PNG thumbnails.
///
/// The editor's API requests use [`GlyphWikiClient`] directly so parsing and
/// limits remain explicit. This adapter exists because GPUI's stock
/// `Application::new()` intentionally installs a null HTTP client.
pub struct GlyphWikiHttpClient {
    /// Connection pool shared with API requests.
    agent: ureq::Agent,
    /// Stable header value returned through GPUI's client interface.
    user_agent: http::HeaderValue,
}

impl GlyphWikiHttpClient {
    /// Creates the native thumbnail transport.
    #[must_use]
    pub fn new() -> Self {
        Self {
            agent: shared_agent().clone(),
            user_agent: http::HeaderValue::from_static(USER_AGENT),
        }
    }
}

impl Default for GlyphWikiHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient for GlyphWikiHttpClient {
    fn type_name(&self) -> &'static str {
        "GlyphWikiHttpClient"
    }

    fn user_agent(&self) -> Option<&http::HeaderValue> {
        Some(&self.user_agent)
    }

    fn send(
        &self,
        request: Request<AsyncBody>,
    ) -> Pin<Box<dyn Future<Output = HttpResult<Response<AsyncBody>>> + Send + 'static>> {
        let (parts, _body) = request.into_parts();
        let method = parts.method;
        let uri = parts.uri;
        let headers = parts.headers;
        let agent = self.agent.clone();

        Box::pin(async move {
            if method != http::Method::GET && method != http::Method::HEAD {
                return Err(anyhow!(
                    "GlyphWiki image transport does not support {method}"
                ));
            }
            if uri.scheme_str() != Some("https") || uri.host() != Some("glyphwiki.org") {
                return Err(anyhow!(
                    "GlyphWiki image transport rejected non-GlyphWiki URI {uri}"
                ));
            }

            smol::unblock(move || {
                let mut builder = http::Request::builder().method(method).uri(uri);
                *builder
                    .headers_mut()
                    .ok_or_else(|| anyhow!("invalid HTTP request builder"))? = headers;
                let request = builder
                    .body(())
                    .map_err(|error| anyhow!(error.to_string()))?;
                let mut response = agent
                    .run(request)
                    .map_err(|error| anyhow!(error.to_string()))?;
                let bytes = response
                    .body_mut()
                    .with_config()
                    .limit(MAX_IMAGE_BODY_BYTES)
                    .read_to_vec()
                    .map_err(|error| anyhow!(error.to_string()))?;
                let (parts, _) = response.into_parts();
                Ok(Response::from_parts(parts, AsyncBody::from(bytes)))
            })
            .await
        })
    }

    fn proxy(&self) -> Option<&Url> {
        None
    }
}

/// Returns the single process-wide HTTPS connection pool.
fn shared_agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        let config = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(6)))
            .timeout_global(Some(Duration::from_secs(15)))
            .http_status_as_error(false)
            .https_only(true)
            .user_agent(USER_AGENT)
            .build();
        config.into()
    })
}

/// Parses a `GlyphWiki` form response and extracts its `data` value.
fn parse_data_field(body: &str) -> Result<String, GlyphWikiError> {
    url::form_urlencoded::parse(body.as_bytes())
        .find_map(|(key, value)| (key == "data").then(|| value.into_owned()))
        .ok_or(GlyphWikiError::MissingData)
}

/// Parses one search response including `GlyphWiki`'s sentinel values.
fn parse_search_body(body: &str) -> Result<SearchResponse, GlyphWikiError> {
    let data = parse_data_field(body)?;
    match data.as_str() {
        "tooshort" => Ok(SearchResponse::TooShort),
        "nodata" | "" => Ok(SearchResponse::NoData),
        _ => Ok(SearchResponse::Matches(
            data.split('\t')
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .collect(),
        )),
    }
}

/// Collects a bounded random name sample with injectable search for tests.
fn random_names_with(
    limit: usize,
    seed: u64,
    mut search: impl FnMut(&str) -> Result<SearchResponse, GlyphWikiError>,
) -> Result<Vec<String>, GlyphWikiError> {
    let target = limit.min(MAX_RANDOM_NAMES);
    if target == 0 {
        return Ok(Vec::new());
    }

    let mut random = SmallRandom::new(seed);
    let mut prefixes = HashSet::new();
    let mut names = Vec::with_capacity(target);
    let mut seen_names = HashSet::new();
    let mut last_error = None;

    for _ in 0..MAX_RANDOM_SEARCH_REQUESTS {
        let prefix = loop {
            let candidate = random_cjk_prefix(&mut random);
            if prefixes.insert(candidate) {
                break candidate;
            }
        };
        let query = format!("u{prefix:04x}");
        match search(&query) {
            Ok(SearchResponse::Matches(matches)) => {
                for name in matches {
                    if !name.is_empty() && seen_names.insert(name.clone()) {
                        names.push(name);
                    }
                }
            }
            Ok(SearchResponse::TooShort | SearchResponse::NoData) => {}
            Err(error) => last_error = Some(error),
        }
        if names.len() >= target {
            random.shuffle(&mut names);
            names.truncate(target);
            return Ok(names);
        }
    }

    if let Some(error) = last_error {
        return Err(error);
    }
    Err(GlyphWikiError::RandomPoolExhausted {
        requested: target,
        found: names.len(),
    })
}

/// Picks one uniformly distributed sixteen-code-point bucket from CJK ranges.
fn random_cjk_prefix(random: &mut SmallRandom) -> u32 {
    let bucket_count = RANDOM_CJK_PREFIX_RANGES
        .iter()
        .map(|(start, end)| usize::try_from(end - start + 1).expect("CJK range fits usize"))
        .sum::<usize>();
    let mut offset = random.index(bucket_count);
    for &(start, end) in RANDOM_CJK_PREFIX_RANGES {
        let len = usize::try_from(end - start + 1).expect("CJK range fits usize");
        if offset < len {
            return start + u32::try_from(offset).expect("CJK range offset fits u32");
        }
        offset -= len;
    }
    unreachable!("random CJK prefix offset is inside declared ranges")
}

/// Supplies a different seed across rapid calls without adding an RNG crate.
fn random_seed() -> u64 {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    time ^ u64::from(std::process::id()).rotate_left(17)
        ^ SEQUENCE.fetch_add(1, Ordering::Relaxed).rotate_left(37)
}

/// Tiny non-cryptographic generator used only to choose and shuffle UI samples.
struct SmallRandom(u64);

impl SmallRandom {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value >> 12;
        value ^= value << 25;
        value ^= value >> 27;
        self.0 = value;
        value.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn index(&mut self, upper_bound: usize) -> usize {
        debug_assert!(upper_bound > 0);
        (self.next() as usize) % upper_bound
    }

    fn shuffle<T>(&mut self, values: &mut [T]) {
        for upper in (1..values.len()).rev() {
            values.swap(upper, self.index(upper + 1));
        }
    }
}

/// Resolves a complete component closure with an injectable source fetcher.
fn resolve_component_tree_with(
    root: &str,
    known: &ComponentLibrary,
    mut fetch: impl FnMut(&str) -> Result<String, GlyphWikiError>,
) -> Result<Vec<ComponentDefinition>, GlyphWikiError> {
    let root = root.trim();
    if root.is_empty() {
        return Err(GlyphWikiError::NotFound(String::new()));
    }

    let mut visiting = BTreeSet::new();
    let mut resolved = HashSet::new();
    let mut definitions = Vec::new();
    resolve_component(
        root,
        0,
        known,
        &mut fetch,
        &mut visiting,
        &mut resolved,
        &mut definitions,
    )?;
    Ok(definitions)
}

/// Depth-first resolver that catches cycles before committing definitions.
fn resolve_component(
    name: &str,
    depth: usize,
    known: &ComponentLibrary,
    fetch: &mut impl FnMut(&str) -> Result<String, GlyphWikiError>,
    visiting: &mut BTreeSet<String>,
    resolved: &mut HashSet<String>,
    definitions: &mut Vec<ComponentDefinition>,
) -> Result<(), GlyphWikiError> {
    if known.get(name).is_some() || resolved.contains(name) {
        return Ok(());
    }
    if depth > MAX_COMPONENT_DEPTH || definitions.len() + visiting.len() >= MAX_COMPONENTS_PER_LOAD
    {
        return Err(GlyphWikiError::DependencyLimit);
    }
    if !visiting.insert(name.to_owned()) {
        return Err(GlyphWikiError::DependencyCycle(name.to_owned()));
    }

    let source = fetch(name)?;
    let definition =
        ComponentDefinition::new(name, name, std::iter::empty::<String>(), source.clone());
    parse_kage(definition.source()).map_err(|error| GlyphWikiError::InvalidSource {
        name: name.to_owned(),
        message: error.to_string(),
    })?;

    for dependency in referenced_component_names(&source) {
        resolve_component(
            &dependency,
            depth + 1,
            known,
            fetch,
            visiting,
            resolved,
            definitions,
        )?;
    }

    visiting.remove(name);
    resolved.insert(name.to_owned());
    definitions.push(definition);
    Ok(())
}

/// Finds unique type-99 references in source order.
fn referenced_component_names(source: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    source
        .split(['$', '\r', '\n'])
        .filter_map(|record| {
            let fields = record.split(':').collect::<Vec<_>>();
            (fields.first().copied() == Some("99"))
                .then(|| fields.get(7).copied())
                .flatten()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .filter(|name| seen.insert((*name).to_owned()))
                .map(str::to_owned)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::{BTreeMap, VecDeque};

    use super::*;

    #[test]
    fn search_parser_preserves_order_and_decodes_form_values() {
        assert_eq!(
            parse_search_body("data=u6c38%09u6c38-j%09%E6%B0%B8"),
            Ok(SearchResponse::Matches(vec![
                "u6c38".to_owned(),
                "u6c38-j".to_owned(),
                "永".to_owned(),
            ]))
        );
        assert_eq!(
            parse_search_body("ignored=1&data=tooshort"),
            Ok(SearchResponse::TooShort)
        );
        assert_eq!(parse_search_body("data=nodata"), Ok(SearchResponse::NoData));
        assert_eq!(
            parse_search_body("message=missing"),
            Err(GlyphWikiError::MissingData)
        );
    }

    #[test]
    fn random_names_are_bounded_shuffled_and_deduplicated() {
        let responses = RefCell::new(VecDeque::from([
            SearchResponse::Matches(vec!["one".to_owned(), "two".to_owned(), "two".to_owned()]),
            SearchResponse::Matches(vec![
                "two".to_owned(),
                "three".to_owned(),
                "four".to_owned(),
                "five".to_owned(),
                "six".to_owned(),
            ]),
        ]));
        let queries = RefCell::new(Vec::new());
        let names = random_names_with(5, 0x1234, |query| {
            queries.borrow_mut().push(query.to_owned());
            Ok(responses
                .borrow_mut()
                .pop_front()
                .expect("two fake searches are sufficient"))
        })
        .expect("fake random pool");

        assert_eq!(names.len(), 5);
        assert_eq!(names.iter().collect::<HashSet<_>>().len(), 5);
        assert!(names.iter().all(|name| [
            "one", "two", "three", "four", "five", "six"
        ]
        .contains(&name.as_str())));
        let queries = queries.into_inner();
        assert_eq!(queries.len(), 2);
        assert_eq!(queries.iter().collect::<HashSet<_>>().len(), 2);
        assert!(queries.iter().all(|query| {
            query.len() == 5
                && query.starts_with('u')
                && query[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
        }));
    }

    #[test]
    fn random_names_limit_is_capped_at_fifty() {
        let names = random_names_with(usize::MAX, 7, |_| {
            Ok(SearchResponse::Matches(
                (0..80).map(|index| format!("glyph-{index}")).collect(),
            ))
        })
        .expect("one large fake result");

        assert_eq!(names.len(), MAX_RANDOM_NAMES);
        assert_eq!(names.iter().collect::<HashSet<_>>().len(), MAX_RANDOM_NAMES);
    }

    #[test]
    fn random_names_stop_after_the_request_budget() {
        let calls = RefCell::new(0);
        let error = random_names_with(50, 99, |_| {
            *calls.borrow_mut() += 1;
            Ok(SearchResponse::NoData)
        })
        .expect_err("an empty fake index cannot fill the pool");

        assert_eq!(*calls.borrow(), MAX_RANDOM_SEARCH_REQUESTS);
        assert_eq!(
            error,
            GlyphWikiError::RandomPoolExhausted {
                requested: 50,
                found: 0,
            }
        );
    }

    #[test]
    fn zero_random_names_do_not_issue_a_request() {
        let names = random_names_with(0, 1, |_| panic!("zero limit must not search"))
            .expect("empty sample");
        assert!(names.is_empty());
    }

    #[test]
    fn recursive_resolution_loads_alias_dependencies_once() {
        let sources = BTreeMap::from([
            ("u6c38".to_owned(), "99:0:0:0:0:200:200:u6c38-j".to_owned()),
            (
                "u6c38-j".to_owned(),
                "1:0:2:34:60:100:60$99:0:0:0:0:200:200:shared".to_owned(),
            ),
            ("shared".to_owned(), "1:0:0:20:20:180:180".to_owned()),
        ]);
        let calls = RefCell::new(Vec::new());
        let definitions = resolve_component_tree_with("u6c38", &ComponentLibrary::new(), |name| {
            calls.borrow_mut().push(name.to_owned());
            sources
                .get(name)
                .cloned()
                .ok_or_else(|| GlyphWikiError::NotFound(name.to_owned()))
        })
        .expect("complete alias tree");

        assert_eq!(calls.into_inner(), ["u6c38", "u6c38-j", "shared"]);
        assert_eq!(
            definitions
                .iter()
                .map(ComponentDefinition::name)
                .collect::<Vec<_>>(),
            ["shared", "u6c38-j", "u6c38"]
        );
    }

    #[test]
    fn known_dependencies_are_reused_without_network_calls() {
        let known = ComponentLibrary::builtin();
        let definitions = resolve_component_tree_with("u6728", &known, |_| {
            panic!("known test fixtures should not be fetched")
        })
        .expect("known component");
        assert!(definitions.is_empty());
    }

    #[test]
    fn recursive_resolution_rejects_cycles_atomically() {
        let sources = BTreeMap::from([
            ("a".to_owned(), "99:0:0:0:0:200:200:b".to_owned()),
            ("b".to_owned(), "99:0:0:0:0:200:200:a".to_owned()),
        ]);
        let result = resolve_component_tree_with("a", &ComponentLibrary::new(), |name| {
            sources
                .get(name)
                .cloned()
                .ok_or_else(|| GlyphWikiError::NotFound(name.to_owned()))
        });
        assert_eq!(result, Err(GlyphWikiError::DependencyCycle("a".to_owned())));
    }

    #[test]
    fn stretch_metadata_is_preserved_as_definition_metadata() {
        let source = "0:1:0:100:40:100:160$1:0:0:20:20:180:180";
        let definitions =
            resolve_component_tree_with("stretchable", &ComponentLibrary::new(), |_| {
                Ok(source.to_owned())
            })
            .expect("stretch component");
        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].source(), "1:0:0:20:20:180:180");
        assert!(definitions[0].stretch_guide().is_some());
    }

    #[test]
    fn thumbnail_transport_rejects_other_hosts_before_networking() {
        let client = GlyphWikiHttpClient::new();
        let result = smol::block_on(client.get(
            "https://example.com/not-allowed.png",
            AsyncBody::default(),
            true,
        ));
        assert!(result.is_err());
    }

    #[test]
    #[ignore = "requires live access to glyphwiki.org"]
    fn live_glyphwiki_smoke_test() {
        let client = GlyphWikiClient::new();
        let random_names = client.random_names(50).expect("live random names");
        assert_eq!(random_names.len(), 50);
        assert_eq!(
            random_names.iter().collect::<HashSet<_>>().len(),
            random_names.len()
        );

        let SearchResponse::Matches(names) = client.search("u6c38").expect("live search") else {
            panic!("u6c38 should have live GlyphWiki matches");
        };
        assert!(names.iter().any(|name| name == "u6c38-j"));

        let definitions = client
            .load_component_tree("u6c38-j", &ComponentLibrary::new())
            .expect("live source and dependencies");
        let yong = definitions
            .iter()
            .find(|definition| definition.name() == "u6c38-j")
            .expect("resolved yong definition");
        assert_eq!(parse_kage(yong.source()).expect("valid live KAGE").len(), 7);

        let image_client = GlyphWikiHttpClient::new();
        let image =
            smol::block_on(image_client.get(&thumbnail_url("u6c38-j"), AsyncBody::default(), true))
                .expect("live thumbnail request");
        assert!(image.status().is_success());
        let BodyInner::Bytes(bytes) = image.into_body().0 else {
            panic!("thumbnail adapter should buffer its bounded response");
        };
        assert!(bytes.into_inner().len() > 100);
    }
}
