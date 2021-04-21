use crate::client::Client;
use crate::error::BuilderError;
use crate::response::instant::InstantQueryResponse;
use crate::response::range::RangeQueryResponse;
use async_trait::async_trait;
use chrono::DateTime;
use std::fmt;
use std::str::FromStr;

#[async_trait]
pub trait Query<T: for<'de> serde::Deserialize<'de>> {
    fn get_query_params(&self) -> Vec<(&str, &str)>;
    fn get_query_endpoint(&self) -> &str;

    /// Execute a query.
    ///
    /// ```rust
    /// use prometheus_http_query::{Client, Query, RangeQuery, InstantQuery};
    ///
    /// let client: Client = Default::default();
    ///
    /// let query = RangeQuery {
    ///     query: "up",
    ///     start: "2021-04-09T11:30:00.000+02:00",
    ///     end: "2021-04-09T12:30:00.000+02:00",
    ///     step: "5m",
    ///     timeout: None,
    /// };
    /// let response = tokio_test::block_on( async { query.execute(&client).await.unwrap() });
    /// assert!(response.is_success());
    ///
    /// let query = InstantQuery {
    ///     query: "up".to_string(),
    ///     time: None,
    ///     timeout: None,
    /// };
    /// let response = tokio_test::block_on( async { query.execute(&client).await.unwrap() });
    /// assert!(response.is_success());
    /// ```
    async fn execute(&self, client: &Client) -> Result<T, reqwest::Error> {
        let mut url = client.base_url.clone();

        url.push_str(self.get_query_endpoint());

        let params = self.get_query_params();

        let response = client
            .client
            .get(&url)
            .query(params.as_slice())
            .send()
            .await?;

        // NOTE: Can be changed to .map(async |resp| resp.json ...)
        // when async closures are stable.
        match response.error_for_status() {
            Ok(res) => res.json::<T>().await,
            Err(err) => Err(err),
        }
    }
}

#[derive(Debug)]
pub struct InstantQuery {
    pub query: String,
    pub time: Option<String>,
    pub timeout: Option<String>,
}

#[async_trait]
impl Query<InstantQueryResponse> for InstantQuery {
    fn get_query_params(&self) -> Vec<(&str, &str)> {
        let mut params = vec![("query", self.query.as_str())];

        if let Some(t) = &self.time {
            params.push(("time", t.as_str()));
        }

        if let Some(t) = &self.timeout {
            params.push(("timeout", t.as_str()));
        }

        params
    }

    fn get_query_endpoint(&self) -> &str {
        "/query"
    }
}

impl InstantQuery {
    pub fn builder() -> InstantQueryBuilder<'static> {
        InstantQueryBuilder {
            ..Default::default()
        }
    }
}

pub struct RangeQuery<'a> {
    pub query: &'a str,
    pub start: &'a str,
    pub end: &'a str,
    pub step: &'a str,
    pub timeout: Option<&'a str>,
}

#[async_trait]
impl<'a> Query<RangeQueryResponse> for RangeQuery<'a> {
    fn get_query_params(&self) -> Vec<(&str, &str)> {
        let mut params = vec![
            ("query", self.query),
            ("start", self.start),
            ("end", self.end),
            ("step", self.step),
        ];

        if let Some(t) = &self.timeout {
            params.push(("timeout", t));
        }

        params
    }

    fn get_query_endpoint(&self) -> &str {
        "/query_range"
    }
}

#[derive(Debug)]
pub struct InstantQueryBuilder<'b> {
    metric: Option<&'b str>,
    labels: Option<Vec<Label<'b>>>,
    time: Option<String>,
    timeout: Option<Vec<Duration>>,
}

impl<'b> Default for InstantQueryBuilder<'b> {
    fn default() -> Self {
        InstantQueryBuilder {
            metric: None,
            labels: None,
            time: None,
            timeout: None,
        }
    }
}

impl<'b> InstantQueryBuilder<'b> {
    /// Add a metric name to the time series selector.
    ///
    /// ```rust
    /// use prometheus_http_query::{Client, Query, InstantQuery};
    ///
    /// let client: Client = Default::default();
    ///
    /// let query = InstantQuery::builder()
    ///     .metric("up")
    ///     .unwrap()
    ///     .build()
    ///     .unwrap();
    ///
    /// let response = tokio_test::block_on( async { query.execute(&client).await.unwrap() });
    /// assert!(response.is_success());
    /// ```
    ///
    /// Some strings are reserved PromQL keywords and cannot be used in a query (at least not
    /// as a metric name except using the `__name__` label like `{__name__="on"}`).
    ///
    /// ```rust
    /// use prometheus_http_query::{Client, Query, InstantQuery, BuilderError};
    ///
    /// let client: Client = Default::default();
    ///
    /// let query = InstantQuery::builder().metric("group_left");
    ///
    /// assert!(query.is_err());
    /// ```
    pub fn metric(mut self, metric: &'b str) -> Result<Self, BuilderError> {
        match metric {
            "bool" | "on" | "ignoring" | "group_left" | "group_right" => {
                Err(BuilderError::InvalidMetricName)
            }
            _ => {
                self.metric = Some(metric);
                Ok(self)
            }
        }
    }

    /// Add a label matcher that only selects labels that exactly match the provided string.
    /// Label matchers are chainable and label names can even appear multiple times in one query.
    ///
    /// ```rust
    /// use prometheus_http_query::{Client, Query, InstantQuery};
    ///
    /// let client: Client = Default::default();
    ///
    /// let query = InstantQuery::builder()
    ///     .metric("promhttp_metric_handler_requests_total")
    ///     .unwrap()
    ///     .with_label("code", "200")
    ///     .build()
    ///     .unwrap();
    ///
    /// let response = tokio_test::block_on( async { query.execute(&client).await.unwrap() });
    /// assert!(response.is_success());
    /// ```
    pub fn with_label(mut self, label: &'b str, value: &'b str) -> Self {
        if let Some(ref mut labels) = self.labels {
            labels.push(Label::With((label, value)));
        } else {
            self.labels = Some(vec![Label::With((label, value))]);
        }

        self
    }

    /// Add a label matcher that only selects labels that do not match the provided string.
    /// Label matchers are chainable and label names can even appear multiple times in one query.
    ///
    /// ```rust
    /// use prometheus_http_query::{Client, Query, InstantQuery};
    ///
    /// let client: Client = Default::default();
    ///
    /// let query = InstantQuery::builder()
    ///     .metric("promhttp_metric_handler_requests_total")
    ///     .unwrap()
    ///     .without_label("code", "500")
    ///     .build()
    ///     .unwrap();
    ///
    /// let response = tokio_test::block_on( async { query.execute(&client).await.unwrap() });
    /// assert!(response.is_success());
    /// ```
    pub fn without_label(mut self, label: &'b str, value: &'b str) -> Self {
        if let Some(ref mut labels) = self.labels {
            labels.push(Label::Without((label, value)));
        } else {
            self.labels = Some(vec![Label::Without((label, value))]);
        }

        self
    }

    /// Add a label matcher that only selects labels that regex-match the provided string.
    /// Label matchers are chainable and label names can even appear multiple times in one query.
    ///
    /// ```rust
    /// use prometheus_http_query::{Client, Query, InstantQuery};
    ///
    /// let client: Client = Default::default();
    ///
    /// let query = InstantQuery::builder()
    ///     .metric("promhttp_metric_handler_requests_total")
    ///     .unwrap()
    ///     .match_label("code", "400|500")
    ///     .build()
    ///     .unwrap();
    ///
    /// let response = tokio_test::block_on( async { query.execute(&client).await.unwrap() });
    /// assert!(response.is_success());
    /// ```
    pub fn match_label(mut self, label: &'b str, value: &'b str) -> Self {
        if let Some(ref mut labels) = self.labels {
            labels.push(Label::Matches((label, value)));
        } else {
            self.labels = Some(vec![Label::Matches((label, value))]);
        }

        self
    }

    /// Add a label matcher that only selects labels that do not regex-match the provided string.
    /// Label matchers are chainable and label names can even appear multiple times in one query.
    ///
    /// ```rust
    /// use prometheus_http_query::{Client, Query, InstantQuery};
    ///
    /// let client: Client = Default::default();
    ///
    /// let query = InstantQuery::builder()
    ///     .metric("promhttp_metric_handler_requests_total")
    ///     .unwrap()
    ///     .no_match_label("code", "400|500")
    ///     .build()
    ///     .unwrap();
    ///
    /// let response = tokio_test::block_on( async { query.execute(&client).await.unwrap() });
    /// assert!(response.is_success());
    /// ```
    pub fn no_match_label(mut self, label: &'b str, value: &'b str) -> Self {
        if let Some(ref mut labels) = self.labels {
            labels.push(Label::Clashes((label, value)));
        } else {
            self.labels = Some(vec![Label::Matches((label, value))]);
        }

        self
    }

    /// Evaluate a query at a specific point in time. `time` must be either a UNIX timestamp
    /// with optional decimal places or a RFC3339-compatible timestamp which is passed to the
    /// function as a string literal, e.g. `1618922012` or `2021-04-20T14:33:32+02:00`.
    ///
    /// ```rust
    /// use prometheus_http_query::{Client, Query, InstantQuery};
    ///
    /// let client: Client = Default::default();
    ///
    /// let query = InstantQuery::builder()
    ///     .metric("promhttp_metric_handler_requests_total")
    ///     .unwrap()
    ///     .with_label("code", "200")
    ///     .at("1618922012")
    ///     .unwrap()
    ///     .build()
    ///     .unwrap();
    ///
    /// let response = tokio_test::block_on( async { query.execute(&client).await.unwrap() });
    /// assert!(response.is_success());
    ///
    /// let another_query = InstantQuery::builder()
    ///     .metric("promhttp_metric_handler_requests_total")
    ///     .unwrap()
    ///     .with_label("code", "200")
    ///     .at("2021-04-20T14:33:32+02:00")
    ///     .unwrap()
    ///     .build()
    ///     .unwrap();
    ///
    /// let another_response = tokio_test::block_on( async { another_query.execute(&client).await.unwrap() });
    /// assert!(another_response.is_success());
    /// ```
    pub fn at(mut self, time: &'b str) -> Result<Self, BuilderError> {
        match f64::from_str(time) {
            Ok(t) => self.time = Some(t.to_string()),
            Err(_) => match DateTime::parse_from_rfc3339(time) {
                Ok(t) => self.time = Some(t.to_rfc3339()),
                Err(_) => return Err(BuilderError::InvalidTimeSpecifier),
            },
        }
        Ok(self)
    }

    /// Provide a custom evaluation timeout other than the Prometheus server's
    /// default. Must adhere to the PromQL [time duration format](https://prometheus.io/docs/prometheus/latest/querying/basics/#time_durations).
    ///
    /// ```rust
    /// use prometheus_http_query::{Client, Query, InstantQuery};
    ///
    /// let client: Client = Default::default();
    ///
    /// let query = InstantQuery::builder()
    ///     .metric("promhttp_metric_handler_requests_total")
    ///     .unwrap()
    ///     .with_label("code", "200")
    ///     .timeout("30s500ms")
    ///     .unwrap()
    ///     .build()
    ///     .unwrap();
    ///
    /// let response = tokio_test::block_on( async { query.execute(&client).await.unwrap() });
    /// assert!(response.is_success());
    /// ```
    pub fn timeout(mut self, timeout: &'b str) -> Result<Self, BuilderError> {
        let chars = ['s', 'm', 'h', 'd', 'w', 'y'];

        let durations: Result<Vec<Duration>, BuilderError> = timeout
            .split_inclusive(chars.as_ref())
            .map(|s| s.split_inclusive("ms"))
            .flatten()
            .map(|d| {
                if d.ends_with("ms") {
                    match d.strip_suffix("ms").unwrap().parse::<usize>() {
                        Ok(num) => Ok(Duration::Milliseconds(num)),
                        Err(_) => Err(BuilderError::InvalidTimeDuration),
                    }
                } else if d.ends_with('s') {
                    match d.strip_suffix('s').unwrap().parse::<usize>() {
                        Ok(num) => Ok(Duration::Seconds(num)),
                        Err(_) => Err(BuilderError::InvalidTimeDuration),
                    }
                } else if d.ends_with('m') {
                    match d.strip_suffix('m').unwrap().parse::<usize>() {
                        Ok(num) => Ok(Duration::Minutes(num)),
                        Err(_) => Err(BuilderError::InvalidTimeDuration),
                    }
                } else if d.ends_with('h') {
                    match d.strip_suffix('h').unwrap().parse::<usize>() {
                        Ok(num) => Ok(Duration::Hours(num)),
                        Err(_) => Err(BuilderError::InvalidTimeDuration),
                    }
                } else if d.ends_with('d') {
                    match d.strip_suffix('d').unwrap().parse::<usize>() {
                        Ok(num) => Ok(Duration::Days(num)),
                        Err(_) => Err(BuilderError::InvalidTimeDuration),
                    }
                } else if d.ends_with('w') {
                    match d.strip_suffix('w').unwrap().parse::<usize>() {
                        Ok(num) => Ok(Duration::Weeks(num)),
                        Err(_) => Err(BuilderError::InvalidTimeDuration),
                    }
                } else if d.ends_with('y') {
                    match d.strip_suffix('y').unwrap().parse::<usize>() {
                        Ok(num) => Ok(Duration::Years(num)),
                        Err(_) => Err(BuilderError::InvalidTimeDuration),
                    }
                } else {
                    return Err(BuilderError::InvalidTimeDuration);
                }
            })
            .collect();

        if let Ok(mut d) = durations {
            d.sort_unstable();
            self.timeout = Some(d);
        }

        Ok(self)
    }

    /// Build the query using the provided parameters.
    ///
    /// ```rust
    /// use prometheus_http_query::{Client, Query, InstantQuery};
    ///
    /// let client: Client = Default::default();
    ///
    /// let query = InstantQuery::builder()
    ///     .metric("promhttp_metric_handler_requests_total")
    ///     .unwrap()
    ///     .with_label("code", "400")
    ///     .with_label("code", "500")
    ///     .at("1618987524")
    ///     .unwrap()
    ///     .timeout("1m30s500ms")
    ///     .unwrap()
    ///     .build()
    ///     .unwrap();
    ///
    /// let response = tokio_test::block_on( async { query.execute(&client).await.unwrap() });
    /// assert!(response.is_success());
    /// ```
    pub fn build(&self) -> Result<InstantQuery, BuilderError> {
        let timeout = match &self.timeout {
            Some(to) => {
                let formatted = to
                    .iter()
                    .map(|x| x.to_string())
                    .collect::<Vec<String>>()
                    .as_slice()
                    .concat();

                Some(formatted)
            }
            None => None,
        };

        let labels = match &self.labels {
            Some(l) => {
                let joined = l
                    .iter()
                    .map(|x| match x {
                        Label::With(pair) => format!("{}=\"{}\"", pair.0, pair.1),
                        Label::Without(pair) => format!("{}!=\"{}\"", pair.0, pair.1),
                        Label::Matches(pair) => format!("{}=~\"{}\"", pair.0, pair.1),
                        Label::Clashes(pair) => format!("{}!~\"{}\"", pair.0, pair.1),
                    })
                    .collect::<Vec<String>>()
                    .as_slice()
                    .join(",");

                Some(joined)
            }
            None => None,
        };

        let query = match self.metric {
            Some(m) => match labels {
                Some(l) => format!("{}{{{}}}", m, l),
                None => m.to_string(),
            },
            None => match labels {
                Some(l) => format!("{{{}}}", l),
                None => return Err(BuilderError::IllegalVectorSelector),
            },
        };

        let q = InstantQuery {
            query: query,
            time: self.time.clone(),
            timeout: timeout,
        };

        Ok(q)
    }
}

#[derive(Debug)]
enum Label<'c> {
    With((&'c str, &'c str)),
    Without((&'c str, &'c str)),
    Matches((&'c str, &'c str)),
    Clashes((&'c str, &'c str)),
}

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq)]
enum Duration {
    Milliseconds(usize),
    Seconds(usize),
    Minutes(usize),
    Hours(usize),
    Days(usize),
    Weeks(usize),
    Years(usize),
}

impl fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Duration::Milliseconds(d) => write!(f, "{}ms", d),
            Duration::Seconds(d) => write!(f, "{}s", d),
            Duration::Minutes(d) => write!(f, "{}m", d),
            Duration::Hours(d) => write!(f, "{}h", d),
            Duration::Days(d) => write!(f, "{}d", d),
            Duration::Weeks(d) => write!(f, "{}w", d),
            Duration::Years(d) => write!(f, "{}y", d),
        }
    }
}
