use reqwest::Client;

use crate::config::EmbedConfig;

pub fn build_client(config: &EmbedConfig) -> reqwest::Result<Client> {
    reqwest::Client::builder()
        .timeout(config.timeout)
        .build()
}
