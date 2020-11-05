use std::io::SeekFrom;
use std::result::Result::Ok;
use std::str::FromStr;

use hyper::{Body, body::HttpBody as _, Client, Method, Uri};
use hyper::client::HttpConnector;
use hyper::Request;
use hyper_tls::HttpsConnector;
use tokio::io::{AsyncWriteExt, Result};

use crate::commons::{OptionConvert, StdResAutoConvert};

mod commons;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/73.0.3683.103 Safari/537.36";

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args();
    args.next();

    let count = args.next().option_to_res("Command error")?;
    let count = usize::from_str(&count).res_auto_convert()?;
    let target = args.next().option_to_res("Command error")?;

    let path = std::path::Path::new(&target);
    let file_name = path.file_name().option_to_res("Parse error")?.to_str().option_to_res("Parse error")?;
    let uri = Uri::from_str(&target).res_auto_convert()?;

    let https = HttpsConnector::new();
    let client = Client::builder()
        .build::<_, hyper::Body>(https);

    let file_len = get_file_len(uri.clone(), &client).await?;
    let block = file_len / count;
    let mut future_list = Vec::with_capacity(count + 1);

    for i in 0..count {
        let begin_index = i * block;
        let end_index = if i == count - 1 {
            file_len
        } else {
            (i + 1) * block
        };

        let future = download(uri.clone(),
                              file_name.to_string(),
                              begin_index, end_index,
                              client.clone());
        future_list.push(future);
    }

    futures::future::try_join_all(future_list).await?;
    Ok(())
}

async fn download(uri: Uri,
                  file_name: String,
                  begin_index: usize, end_index: usize,
                  client: Client<HttpsConnector<HttpConnector>>) -> Result<()> {
    let req = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header("User-agent", USER_AGENT)
        .header("Range", format!("bytes={}-{}", begin_index, end_index - 1))
        .body(Body::empty())
        .res_auto_convert()?;

    let mut res = client.request(req).await.res_auto_convert()?;

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(file_name)
        .await?;

    file.seek(SeekFrom::Start(begin_index as u64)).await?;

    while let Some(next) = res.data().await {
        let chunk = next.res_auto_convert()?;
        file.write_all(&chunk).await?;
    }
    Ok(())
}

async fn get_file_len(uri: Uri, client: &Client<HttpsConnector<HttpConnector>>) -> Result<usize> {
    let req = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header("user-agent", USER_AGENT)
        .body(Body::empty())
        .res_auto_convert()?;

    let res = client.request(req).await.res_auto_convert()?;
    let headers = res.headers();
    let len = headers.get("content-length").option_to_res("Get file size error")?;
    let len = usize::from_str(len.to_str().res_auto_convert()?).res_auto_convert()?;

    Ok(len)
}
