use std::{future::Future, pin::Pin, sync::Arc, thread, time::Duration};

use anyhow::Context as _;
use async_channel::bounded;
use gpui_kit::http_client::{HttpClient, HttpResponse};
use reqwest::{blocking::Client, redirect::Policy};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub struct ReqwestClient {
    following: Arc<Client>,
    direct: Arc<Client>,
}

impl ReqwestClient {
    pub fn new(user_agent: &str) -> anyhow::Result<Self> {
        let build = |policy| {
            Client::builder()
                .user_agent(user_agent)
                .redirect(policy)
                .timeout(REQUEST_TIMEOUT)
                .build()
        };
        Ok(Self {
            following: Arc::new(build(Policy::default())?),
            direct: Arc::new(build(Policy::none())?),
        })
    }
}

impl HttpClient for ReqwestClient {
    fn get(
        &self,
        url: &str,
        follow_redirects: bool,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<HttpResponse>> + Send>> {
        let client = if follow_redirects {
            self.following.clone()
        } else {
            self.direct.clone()
        };
        let url = url.to_owned();
        let (tx, rx) = bounded(1);
        let spawned = thread::Builder::new()
            .name("story-http".into())
            .spawn(move || {
                let response = client
                    .get(&url)
                    .send()
                    .and_then(|response| {
                        let status = response.status();
                        response.bytes().map(|body| HttpResponse {
                            status,
                            body: body.to_vec(),
                        })
                    })
                    .with_context(|| format!("requesting {url}"));
                tx.send_blocking(response).ok();
            });
        Box::pin(async move {
            spawned.context("spawning the request thread")?;
            rx.recv().await.context("the request thread stopped")?
        })
    }
}
