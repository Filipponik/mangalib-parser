use crate::mangalib;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tracing::{error, info};

#[derive(Clone)]
struct AppState {
    port: u16,
    chrome_max_count: u16,
}

macro_rules! retry {
    ($f:expr, $count:expr) => {{
        let mut tries = 0;
        let result = loop {
            let result = $f;
            tries += 1;
            if result.is_ok() || tries >= $count {
                break result;
            }
        };
        result
    }};
    ($f:expr) => {
        retry!($f, 5)
    };
}

pub async fn serve() {
    let state = AppState {
        port: env::var("APP_PORT").unwrap().parse::<u16>().unwrap(),
        chrome_max_count: env::var("CHROME_MAX_COUNT")
            .unwrap()
            .parse::<u16>()
            .unwrap(),
    };
    let address = &format!("0.0.0.0:{}", state.port.clone());
    let listener = TcpListener::bind(address).await.unwrap();
    let router: Router = Router::new()
        .route("/scrap-manga", post(scrap_manga))
        .route("/scrap-manga/", post(scrap_manga))
        .with_state(state)
        .fallback(handle_404);

    info!("Web server is up: {address}");
    axum::serve(listener, router).await.unwrap();
}

#[derive(Deserialize)]
struct ScrapMangaRequest {
    slug: String,
    callback_url: String,
}

async fn scrap_manga(
    State(state): State<AppState>,
    Json(payload): Json<ScrapMangaRequest>,
) -> (StatusCode, Json<Value>) {
    tokio::spawn(async move {
        let manga = get_manga_urls(&payload.slug, state.chrome_max_count).await;
        info!("Sending manga to {}", payload.callback_url);
        let response = send_info_about_manga(&payload.callback_url, &manga).await;
        match response {
            Ok(body) => info!("Successfully sent manga: {body}"),
            Err(err) => error!("Error while sending manga: {err:?}"),
        }
    });

    (
        StatusCode::OK,
        Json(json!({
            "success": true,
            "message": "Manga was sent successfully"
        })),
    )
}

async fn handle_404() -> (StatusCode, Json<Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "success": false,
            "message": "Route not found"
        })),
    )
}

async fn get_manga_urls(slug: &str, chrome_max_count: u16) -> PublishedManga {
    let chapter_urls_map: Arc<Mutex<HashMap<mangalib::MangaChapter, Vec<String>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let mut chapters = mangalib::get_manga_chapters(slug).await.unwrap();
    chapters.reverse();
    let mut threads = vec![];
    let semaphore = Arc::new(Semaphore::new(chrome_max_count as usize));
    for chapter in chapters.clone() {
        let urls = Arc::clone(&chapter_urls_map);
        let slug = slug.to_string();
        let semaphore = semaphore.clone();
        let thread = tokio::spawn(async move {
            let _permit = semaphore.acquire().await.unwrap();
            let result = retry!(mangalib::get_manga_chapter_images(&slug, &chapter).await).unwrap();
            let mut urls = urls.lock().unwrap();
            urls.insert(chapter.clone(), result);
        });

        threads.push(thread);
    }

    futures::future::join_all(threads).await;
    let chapter_urls_map = chapter_urls_map.lock().unwrap().clone();

    publish_manga(slug, &chapters, &chapter_urls_map).await
}

#[derive(Debug, Serialize, Deserialize)]
struct PublishedManga {
    slug: String,
    chapters: Vec<PublishedMangaChapter>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PublishedMangaChapter {
    url: Option<String>,
    chapter: String,
    volume: String,
    images_urls: Vec<String>,
}

async fn publish_manga(
    slug: &str,
    chapters: &[mangalib::MangaChapter],
    chapter_urls_map: &HashMap<mangalib::MangaChapter, Vec<String>>,
) -> PublishedManga {
    let mut telegraph_urls: Vec<PublishedMangaChapter> = vec![];
    for chapter in chapters {
        let url_images = chapter_urls_map.get(chapter).unwrap();
        telegraph_urls.push(PublishedMangaChapter {
            url: None,
            chapter: chapter.chapter_number.clone(),
            volume: chapter.chapter_volume.clone(),
            images_urls: url_images.clone(),
        });
    }

    PublishedManga {
        slug: slug.to_string(),
        chapters: telegraph_urls,
    }
}

async fn send_info_about_manga(url: &str, manga: &PublishedManga) -> reqwest::Result<String> {
    reqwest::Client::new()
        .post(url)
        .json(manga)
        .send()
        .await?
        .text()
        .await
}