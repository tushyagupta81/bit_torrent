use std::error::Error;
use reqwest;

pub async fn get_peers(url:String) -> Result<Vec<u8>, Box<dyn Error>> {
    let data = reqwest::get(url).await?;
    let str_data = data.bytes().await?;

    return Ok(str_data.to_vec());
}
