use std::io::SeekFrom;
use std::result::Result::Ok;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

use hyper::{Body, body::HttpBody as _, Client, Method, Uri};
use hyper::client::HttpConnector;
use hyper::Request;
use hyper_tls::HttpsConnector;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tokio::io::{AsyncWriteExt, Result};
use tokio::io::BufWriter;
use tokio::time;

use crate::commons::{OptionConvert, StdResAutoConvert};

mod commons;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/73.0.3683.103 Safari/537.36";
static TOTAL_COUNT: AtomicU64 = AtomicU64::new(0);

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args();
    args.next();

    let count = args.next().option_to_res("Command error")?;
    let count = u64::from_str(&count).res_auto_convert()?;
    let target = args.next().option_to_res("Command error")?;

    let file_name = match args.next() {
        Some(file_name) => file_name,
        None => {
            let path = std::path::Path::new(&target);
            path.file_name().option_to_res("File name Parse error")?.to_str().option_to_res("File name Parse error")?.to_string()
        }
    };

    let uri = Uri::from_str(&target).res_auto_convert()?;

    let https = HttpsConnector::new();
    let client = Client::builder()
        .build::<_, hyper::Body>(https);

    let file_len = get_file_len(uri.clone(), &client).await?;
    println!("File total size: {} MB", file_len / (1024 * 1024));
    let block = file_len / count;

    let multi_progress = MultiProgress::new();

    let pb = multi_progress.add(ProgressBar::new(file_len));
    let style = ProgressStyle::default_bar()
        .template("[{bar:80.while/white}] {bytes}/{total_bytes} {bytes_per_sec} [{elapsed_precise}]")
        .progress_chars("#>-");
    pb.set_style(style);
    total_count_draw(pb, file_len);

    let mut future_list = Vec::with_capacity(count as usize);

    let style = ProgressStyle::default_bar()
        .template("[{bar:40.cyan/blue}] {bytes}/{total_bytes} {bytes_per_sec} ({eta})")
        .progress_chars("#>-");

    for i in 0..count {
        let begin_index = i * block;
        let end_index = if i == count - 1 {
            file_len
        } else {
            (i + 1) * block
        };

        let pb = multi_progress.add(ProgressBar::new(end_index - begin_index));
        pb.set_style(style.clone());

        let future = download(uri.clone(),
                              file_name.to_string(),
                              begin_index, end_index,
                              client.clone(),
                              pb);
        future_list.push(future);
    }

    tokio::task::spawn_blocking(move || multi_progress.join_and_clear());
    futures::future::try_join_all(future_list).await?;
    Ok(())
}

fn total_count_draw(pb: ProgressBar, file_len: u64) {
    tokio::spawn(async move {
        let mut interval = time::interval(time::Duration::from_millis(500));

        loop {
            let total_count = TOTAL_COUNT.load(Ordering::SeqCst);

            if total_count >= file_len {
                pb.finish_with_message("done");
                return;
            } else {
                pb.set_position(total_count);
            }
            interval.tick().await;
        }
    });
}

async fn download(uri: Uri,
                  file_name: String,
                  begin_index: u64, end_index: u64,
                  client: Client<HttpsConnector<HttpConnector>>,
                  pb: ProgressBar) -> Result<()> {
    let mut count = 0;

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(file_name)
        .await?;

    file.seek(SeekFrom::Start(begin_index)).await?;
    let mut file = BufWriter::new(file);

    loop {
        let req = Request::builder()
            .method(Method::GET)
            .uri(uri.clone())
            .header("User-agent", USER_AGENT)
            .header("Range", format!("bytes={}-{}", begin_index + count, end_index - 1))
            .body(Body::empty())
            .res_auto_convert()?;

        let mut res = client.request(req).await.res_auto_convert()?;

        loop {
            match res.data().await {
                Some(next) => {
                    let chunk = match next {
                        Ok(buff) => buff,
                        Err(_) => break
                    };
                    file.write_all(&chunk).await?;

                    let len = chunk.len() as u64;
                    count += len;
                    TOTAL_COUNT.fetch_add(len, Ordering::SeqCst);
                    pb.set_position(count);
                }
                None => {
                    file.flush().await?;
                    pb.finish_with_message("done");
                    return Ok(());
                }
            }
        }
    }
}

async fn get_file_len(uri: Uri, client: &Client<HttpsConnector<HttpConnector>>) -> Result<u64> {
    let req = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header("user-agent", USER_AGENT)
        .body(Body::empty())
        .res_auto_convert()?;

    let res = client.request(req).await.res_auto_convert()?;
    let headers = res.headers();
    let len = headers.get("content-length").option_to_res("Get file size error")?;
    let len = u64::from_str(len.to_str().res_auto_convert()?).res_auto_convert()?;

    Ok(len)
}
