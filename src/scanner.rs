use anyhow::Result;
use futures::StreamExt;
use log::{debug, error, info};
use reqwest::{Client, StatusCode, Url};
use scraper::{Html, Selector};
use std::{collections::HashSet, time::Duration};
use tokio::sync::mpsc::{self, Receiver, Sender};

enum CrawlEvent {
    Success {
        crawled_url: String,
        status_code: StatusCode,
        collected_urls: Vec<String>,
    },
    Failed {
        crawled_url: String,
        status_code: StatusCode,
    },
    Timeout {
        crawled_url: String,
    },
}

pub struct Scanner {
    http_client: Client,
    concurrency: u32,
    /// All collected URL since the crawler is running.
    collected_urls: HashSet<String>,
}

impl Scanner {
    pub fn new(timeout: Duration, connect_timeout: Duration, concurrency: u32) -> Result<Self> {
        let http_client = Client::builder()
            .connect_timeout(connect_timeout)
            .timeout(timeout)
            .build()?;

        Ok(Self {
            http_client,
            concurrency,
            collected_urls: HashSet::new(),
        })
    }

    pub async fn run(&mut self, start_url: Url) -> Result<()> {
        info!("start scanning");

        let base_scheme = start_url.scheme();
        // unwrap is safe here as start_url is validated in main
        let host = start_url.host().unwrap();

        let (crawling_queue_tx, crawling_queue_rx) = mpsc::channel(100);
        let (collector_queue_tx, mut collector_queue_rx) = mpsc::channel::<CrawlEvent>(100);

        self.add_to_crawl_queue(crawling_queue_tx.clone(), start_url.to_string())
            .await?;

        // Task for crawling URLs
        self.start_crawling_task(
            self.http_client.clone(),
            self.concurrency as usize,
            crawling_queue_rx,
            collector_queue_tx.clone(),
        );

        loop {
            tokio::select! {
                res = collector_queue_rx.recv() => {
                    if let Some(event) = res {
                        match event {
                            CrawlEvent::Success { crawled_url, status_code, collected_urls } => {
                                println!("{} -> {}", status_code, crawled_url);

                                let sanitized_urls: Vec<String> = sanitize_urls(collected_urls, base_scheme, host.to_string());

                                for url in sanitized_urls {
                                    if let Err(err) =
                                            self.add_to_crawl_queue(crawling_queue_tx.clone(), url.clone()).await
                                        {
                                            error!("cannot add {} to the crawl queue: {}", url, err);
                                        }
                                }
                            },
                            CrawlEvent::Failed { crawled_url, status_code } => {
                                println!("{} -> {}", status_code, crawled_url);
                            },
                            CrawlEvent::Timeout { crawled_url } => {
                                println!("Timeout -> {}", crawled_url);
                            },
                        }

                        // TODO: make it a param
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
                else => {
                    break;
                }
            }
        }

        Ok(())
    }

    async fn add_to_crawl_queue(
        &mut self,
        crawling_queue_tx: Sender<String>,
        url: String,
    ) -> Result<()> {
        if !self.collected_urls.contains(&url) {
            debug!("URL '{}' added to the crawl queue", url);
            self.collected_urls.insert(url.clone());
            crawling_queue_tx.send(url).await?;
        }
        Ok(())
    }

    fn start_crawling_task(
        &self,
        http_client: Client,
        crawling_concurrency: usize,
        crawling_queue_rx: Receiver<String>,
        collector_queue_tx: Sender<CrawlEvent>,
    ) {
        info!("crawling task started");

        tokio::spawn(async move {
            tokio_stream::wrappers::ReceiverStream::new(crawling_queue_rx)
                .for_each_concurrent(crawling_concurrency, |pending_url| {
                    info!("scanning URL: {}", pending_url);
                    let pending_url = pending_url.clone();

                    async {
                        let response = http_client.get(&pending_url).send().await;

                        match response {
                            Ok(resp) => {
                                let status_code = resp.status();
                                match resp.text().await {
                                    Ok(html) => {
                                        let found_urls = scrap_links(html);

                                        let msg = CrawlEvent::Success {
                                            crawled_url: pending_url,
                                            status_code,
                                            collected_urls: found_urls,
                                        };
                                        if let Err(err) = collector_queue_tx.send(msg).await {
                                            error!("collector sender dropped: {}", err);
                                        }
                                    }
                                    Err(err) => {
                                        error!("cannot decode response body: {}", err);
                                        let msg = CrawlEvent::Failed {
                                            crawled_url: pending_url,
                                            status_code: status_code,
                                        };
                                        if let Err(err) = collector_queue_tx.send(msg).await {
                                            error!("collector sender dropped: {}", err);
                                        }
                                    }
                                }
                            }
                            Err(err) => {
                                error!("cannot scan URL: {}", err);
                                // TODO: make this event more precise
                                let msg = CrawlEvent::Timeout {
                                    crawled_url: pending_url,
                                };
                                if let Err(err) = collector_queue_tx.send(msg).await {
                                    error!("collector sender dropped: {}", err);
                                }
                            }
                        };
                    }
                })
                .await;
        });
    }
}

/// Scrap anchor tags in HTML and extract href attribute content
fn scrap_links(html: String) -> Vec<String> {
    let document = Html::parse_document(&html);
    let selector = Selector::parse("a").unwrap();

    // collect all links
    let found_urls = document
        .select(&selector)
        .filter_map(|el| el.value().attr("href"))
        .map(|link| link.to_string())
        .collect();

    found_urls
}

/// sanitize and normalize collected URLs
/// - Remove anchor links
/// - handle protocol free URLs and relative URLs
fn sanitize_urls(urls: Vec<String>, scheme: &str, host: String) -> Vec<String> {
    urls.iter()
        .filter(|url| !url.starts_with("#"))
        .map(|url| {
            // protocol free URL
            if url.starts_with("//") {
                return format!("{}:{}", scheme, url);
            }

            // relative URL
            if url.starts_with("/") {
                return format!("{}://{}{}", scheme, host, url);
            }

            url.to_string()
        })
        .filter(|url| {
            url.starts_with(&format!("http://{}", host))
                || url.starts_with(&format!("https://{}", host))
        })
        .collect()
}
