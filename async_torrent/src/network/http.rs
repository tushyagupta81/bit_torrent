use std::error::Error;
use reqwest;

pub async fn get_peers(url:String) -> Result<Vec<u8>, Box<dyn Error + Send+Sync>> {
    let data = reqwest::get(url).await?;
    let str_data = data.bytes().await?;

    Ok(str_data.to_vec())
}
