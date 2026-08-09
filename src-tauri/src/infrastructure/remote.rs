use crate::domain::{
    MediaServerConfig, MediaServerCredentials, MediaServerInput, RemoteEpisodeDetail,
    RemoteLibraryEntry, RemoteMediaDetail, RemotePersonDetail, RemoteRecentMedia,
    RemoteSeasonDetail, natural_cmp,
};
use crate::error::{AppError, AppResult};
use base64::Engine;
use chrono::DateTime;
use reqwest::blocking::{Client, Response};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;

const TOKEN_HEADER: &str = "X-Emby-Token";
const EMBY_AUTHORIZATION_HEADER: &str = "X-Emby-Authorization";
const CLIENT_NAME: &str = "mpv-enjoy Home";
const DEVICE_NAME: &str = "Desktop";
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
const AUTHENTICATION_REQUIRED_MARKER: &str = "REMOTE_AUTHENTICATION_REQUIRED:";

#[derive(Debug, Clone)]
pub struct VerifiedServer {
    pub kind: String,
    pub name: String,
    pub base_url: String,
    pub token: String,
    pub user_id: String,
    pub user_name: String,
    pub server_version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RemotePlayback {
    pub items: Vec<RemotePlaybackItem>,
    pub start_index: usize,
    pub reporter: Option<RemotePlaybackReporter>,
}

#[derive(Debug, Clone)]
pub struct RemotePlaybackItem {
    pub item_id: String,
    pub media_source_id: Option<String>,
    pub url: String,
    pub title: String,
    pub subtitle_urls: Vec<String>,
    pub audio_urls: Vec<String>,
    pub start_position_ticks: i64,
    pub run_time_ticks: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct RemotePlaybackReporter {
    remote: RemoteClient,
    play_session_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackReportKind {
    Started,
    Progress,
    Stopped,
}

#[derive(Debug, Clone, Copy)]
pub struct PlaybackState {
    pub position_seconds: f64,
    pub duration_seconds: Option<f64>,
    pub paused: bool,
    pub muted: bool,
    pub volume: Option<f64>,
    pub playback_rate: f64,
}

#[derive(Debug, Clone)]
pub struct RemoteClient {
    client: Client,
    api_base: Url,
    kind: String,
    token: String,
    user_id: String,
    device_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SystemInfo {
    server_name: Option<String>,
    product_name: Option<String>,
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteUser {
    id: String,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct AuthenticateRequest<'a> {
    username: &'a str,
    pw: &'a str,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AuthenticationResult {
    user: RemoteUser,
    access_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct QueryResult {
    #[serde(default)]
    items: Vec<BaseItem>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct BaseItem {
    id: String,
    name: String,
    #[serde(rename = "Type", default)]
    item_type: String,
    #[serde(default)]
    is_folder: bool,
    media_type: Option<String>,
    collection_type: Option<String>,
    overview: Option<String>,
    #[serde(default)]
    taglines: Vec<String>,
    #[serde(default)]
    genres: Vec<String>,
    child_count: Option<usize>,
    recursive_item_count: Option<usize>,
    index_number: Option<i32>,
    parent_index_number: Option<i32>,
    series_name: Option<String>,
    series_id: Option<String>,
    date_created: Option<String>,
    production_year: Option<i32>,
    premiere_date: Option<String>,
    run_time_ticks: Option<i64>,
    community_rating: Option<f64>,
    official_rating: Option<String>,
    primary_image_aspect_ratio: Option<f64>,
    user_data: Option<UserItemData>,
    parent_id: Option<String>,
    season_id: Option<String>,
    parent_backdrop_item_id: Option<String>,
    #[serde(default)]
    parent_backdrop_image_tags: Vec<String>,
    #[serde(default)]
    backdrop_image_tags: Vec<String>,
    #[serde(default)]
    people: Vec<Person>,
    #[serde(default)]
    media_sources: Vec<MediaSource>,
    #[serde(default)]
    image_tags: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct MediaSource {
    id: String,
    container: Option<String>,
    run_time_ticks: Option<i64>,
    #[serde(default)]
    media_streams: Vec<MediaStream>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct UserItemData {
    playback_position_ticks: Option<i64>,
    #[serde(default)]
    played: bool,
    played_percentage: Option<f64>,
    unplayed_item_count: Option<usize>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Person {
    id: Option<String>,
    name: String,
    role: Option<String>,
    #[serde(rename = "Type")]
    person_type: Option<String>,
    primary_image_tag: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct PlaybackCheckIn<'a> {
    queueable_media_types: [&'static str; 1],
    can_seek: bool,
    item_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    media_source_id: Option<&'a str>,
    is_paused: bool,
    is_muted: bool,
    position_ticks: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    run_time_ticks: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    volume_level: Option<i32>,
    play_method: &'static str,
    play_session_id: &'a str,
    playlist_index: usize,
    playlist_length: usize,
    playback_rate: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_name: Option<&'static str>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct MediaStream {
    #[serde(rename = "Type", default)]
    stream_type: String,
    index: Option<i32>,
    codec: Option<String>,
    #[serde(default)]
    is_external: bool,
    #[serde(default)]
    is_text_subtitle_stream: bool,
    delivery_url: Option<String>,
}

impl RemoteClient {
    pub fn verify(input: &MediaServerInput, device_id: &str) -> AppResult<VerifiedServer> {
        let kind = normalize_kind(&input.kind)?;
        let base_url = normalize_base_url(&input.base_url)?;
        let mut remote = Self::build(&kind, &base_url, "", "", device_id)?;
        let username = normalize_username(&input.username)?;
        let password = normalize_password(&input.password)?;
        let result = remote
            .authenticate_by_name(&username, &password)
            .map_err(|error| contextual_error("用户名和密码登录失败", error))?;
        let token = normalize_token(&result.access_token)?;
        let user = result.user;

        remote.token.clone_from(&token);
        remote.user_id.clone_from(&user.id);
        let info: SystemInfo = remote
            .get_json(&["System", "Info"], &[])
            .map_err(|error| contextual_error("读取服务器信息失败", error))?;
        let _: QueryResult = remote
            .get_json(&["Users", &user.id, "Views"], &[])
            .map_err(|error| contextual_error("读取用户媒体库失败", error))?;

        let requested_name = input.name.trim();
        let detected_name = info
            .server_name
            .or(info.product_name)
            .unwrap_or_else(|| kind_label(&kind).to_string());
        Ok(VerifiedServer {
            kind,
            name: if requested_name.is_empty() {
                detected_name
            } else {
                requested_name.to_string()
            },
            base_url,
            token,
            user_id: user.id,
            user_name: user.name,
            server_version: info.version,
        })
    }

    pub fn from_config(server: &MediaServerConfig, device_id: &str) -> AppResult<Self> {
        Self::build(
            &server.kind,
            &server.base_url,
            &server.token,
            &server.user_id,
            device_id,
        )
    }

    pub fn reauthenticate(
        server: &MediaServerConfig,
        credentials: &MediaServerCredentials,
        device_id: &str,
    ) -> AppResult<VerifiedServer> {
        Self::verify(
            &MediaServerInput {
                kind: server.kind.clone(),
                name: server.name.clone(),
                base_url: server.base_url.clone(),
                username: credentials.username.clone(),
                password: credentials.password.clone(),
            },
            device_id,
        )
    }

    pub fn list_entries(&self, parent_id: Option<&str>) -> AppResult<Vec<RemoteLibraryEntry>> {
        let items = if let Some(parent_id) = parent_id {
            validate_identifier(parent_id, "远程目录")?;
            let parent: BaseItem = self.get_json(
                &["Users", &self.user_id, "Items", parent_id],
                &[("Fields", "CollectionType")],
            )?;
            let include_types = library_include_types(&parent);
            let query = [
                ("ParentId", parent_id),
                ("Recursive", "true"),
                ("IncludeItemTypes", include_types),
                (
                    "Fields",
                    "PrimaryImageAspectRatio,Overview,ParentId,DateCreated,IndexNumber,ParentIndexNumber,SeriesName,UserData",
                ),
                ("SortBy", "SortName"),
                ("SortOrder", "Ascending"),
                ("EnableImages", "true"),
                ("EnableUserData", "true"),
                ("ImageTypeLimit", "1"),
                ("Limit", "2000"),
            ];
            self.get_json::<QueryResult>(&["Users", &self.user_id, "Items"], &query)?
                .items
        } else {
            self.get_json::<QueryResult>(&["Users", &self.user_id, "Views"], &[])?
                .items
        };

        let mut items = items
            .into_iter()
            .filter(is_video_or_container)
            .collect::<Vec<_>>();
        sort_remote_items(&mut items);
        Ok(items.into_iter().map(to_library_entry).collect())
    }

    pub fn list_recent_media(&self, limit: usize) -> AppResult<Vec<RemoteRecentMedia>> {
        let limit = limit.clamp(1, 24).to_string();
        let query = [
            ("Recursive", "true"),
            ("IncludeItemTypes", "Episode,Movie,Video"),
            (
                "Fields",
                "DateCreated,SeriesName,SeriesId,IndexNumber,ParentIndexNumber",
            ),
            ("SortBy", "DateCreated"),
            ("SortOrder", "Descending"),
            ("EnableImages", "false"),
            ("Limit", limit.as_str()),
        ];
        let items = self
            .get_json::<QueryResult>(&["Users", &self.user_id, "Items"], &query)?
            .items;

        Ok(items.into_iter().filter_map(remote_recent_media).collect())
    }

    pub fn media_detail(&self, item_id: &str) -> AppResult<RemoteMediaDetail> {
        validate_identifier(item_id, "媒体条目")?;
        let item: BaseItem = self.get_json(
            &["Users", &self.user_id, "Items", item_id],
            &[(
                "Fields",
                "Overview,Taglines,Genres,People,PrimaryImageAspectRatio,ParentId,IndexNumber,ParentIndexNumber,SeriesName,PremiereDate,UserData",
            )],
        )?;
        if !matches!(item.item_type.as_str(), "Series" | "Movie" | "Video") {
            return Err(AppError::message("远程条目不支持媒体详情视图"));
        }

        let (mut seasons, mut episodes) = if item.item_type == "Series" {
            let common_fields = "Overview,PrimaryImageAspectRatio,ParentId,IndexNumber,ParentIndexNumber,SeriesName,PremiereDate,UserData";
            let season_query = [
                ("ParentId", item_id),
                ("IncludeItemTypes", "Season"),
                ("Fields", common_fields),
                ("SortBy", "IndexNumber,SortName"),
                ("SortOrder", "Ascending"),
                ("EnableImages", "true"),
                ("EnableUserData", "true"),
            ];
            let episode_query = [
                ("ParentId", item_id),
                ("Recursive", "true"),
                ("IncludeItemTypes", "Episode"),
                ("Fields", common_fields),
                ("SortBy", "ParentIndexNumber,IndexNumber,SortName"),
                ("SortOrder", "Ascending"),
                ("EnableImages", "true"),
                ("EnableUserData", "true"),
                ("Limit", "2000"),
            ];
            let seasons = self
                .get_json::<QueryResult>(&["Users", &self.user_id, "Items"], &season_query)?
                .items;
            let episodes = self
                .get_json::<QueryResult>(&["Users", &self.user_id, "Items"], &episode_query)?
                .items;
            (seasons, episodes)
        } else {
            (Vec::new(), Vec::new())
        };

        sort_remote_items(&mut seasons);
        sort_remote_items(&mut episodes);
        let season_details = seasons
            .iter()
            .map(|season| season_detail(season, &episodes))
            .collect();
        let episode_details = episodes.iter().map(episode_detail).collect();
        Ok(media_detail(item, season_details, episode_details))
    }

    pub fn image_data_url(
        &self,
        item_id: &str,
        image_type: &str,
        max_width: u32,
    ) -> AppResult<Option<String>> {
        validate_identifier(item_id, "媒体条目")?;
        if !matches!(image_type, "Primary" | "Backdrop") {
            return Err(AppError::message("不支持的远程图片类型"));
        }
        let max_width = max_width.clamp(80, 1920).to_string();
        let mut segments = vec!["Items", item_id, "Images", image_type];
        if image_type == "Backdrop" {
            segments.push("0");
        }
        let response = self.send_get(
            &segments,
            &[("MaxWidth", max_width.as_str()), ("Quality", "82")],
        )?;
        if response.status().as_u16() == 404 {
            return Ok(None);
        }
        let response = ensure_success(response)?;
        let mime_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .filter(|value| value.starts_with("image/"))
            .unwrap_or("image/jpeg")
            .to_string();
        let bytes = response
            .bytes()
            .map_err(|error| remote_error("读取远程封面失败", error))?;
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err(AppError::message("远程图片超过 8 MiB 限制"));
        }
        Ok(Some(format!(
            "data:{mime_type};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )))
    }

    pub fn playback(&self, item_id: &str) -> AppResult<RemotePlayback> {
        validate_identifier(item_id, "媒体条目")?;
        let selected: BaseItem = self.get_json(
            &["Users", &self.user_id, "Items", item_id],
            &[("Fields", "MediaSources,UserData,RunTimeTicks")],
        )?;
        if selected.is_folder || selected.media_type.as_deref() != Some("Video") {
            return Err(AppError::message("远程条目不是可播放的视频"));
        }

        let mut queue = if selected.item_type == "Episode" {
            if let Some(series_id) = selected.series_id.as_deref() {
                validate_identifier(series_id, "剧集")?;
                let query = [
                    ("ParentId", series_id),
                    ("Recursive", "true"),
                    ("IncludeItemTypes", "Episode"),
                    ("Fields", "MediaSources,UserData,RunTimeTicks"),
                    ("SortBy", "ParentIndexNumber,IndexNumber,SortName"),
                    ("SortOrder", "Ascending"),
                    ("Limit", "500"),
                ];
                self.get_json::<QueryResult>(&["Users", &self.user_id, "Items"], &query)?
                    .items
            } else {
                vec![selected.clone()]
            }
        } else {
            vec![selected.clone()]
        };

        sort_remote_items(&mut queue);
        if !queue.iter().any(|item| item.id == selected.id) {
            queue = vec![selected.clone()];
        }
        let start_index = queue
            .iter()
            .position(|item| item.id == selected.id)
            .unwrap_or(0);
        let play_session_id = new_play_session_id();
        let items = queue
            .iter()
            .map(|item| self.playback_item(item, &play_session_id))
            .collect::<AppResult<Vec<_>>>()?;
        Ok(RemotePlayback {
            items,
            start_index,
            reporter: Some(RemotePlaybackReporter {
                remote: self.clone(),
                play_session_id,
            }),
        })
    }

    fn playback_item(
        &self,
        item: &BaseItem,
        play_session_id: &str,
    ) -> AppResult<RemotePlaybackItem> {
        validate_identifier(&item.id, "媒体条目")?;
        let source = item.media_sources.first();
        let stream_name = source
            .and_then(|source| source.container.as_deref())
            .and_then(safe_extension)
            .map(|container| format!("stream.{container}"))
            .unwrap_or_else(|| "stream".to_string());
        let mut url = self.endpoint(&["Videos", &item.id, &stream_name])?;
        url.query_pairs_mut()
            .append_pair("DeviceId", &self.device_id)
            .append_pair("PlaySessionId", play_session_id)
            .append_pair("api_key", &self.token)
            .append_pair("Static", "true");
        if let Some(source) = source {
            validate_identifier(&source.id, "媒体源")?;
            url.query_pairs_mut()
                .append_pair("MediaSourceId", &source.id);
        }

        let mut subtitle_urls = Vec::new();
        let mut audio_urls = Vec::new();
        if let Some(source) = source {
            for stream in &source.media_streams {
                if !stream.is_external {
                    continue;
                }
                let Some(index) = stream.index else {
                    continue;
                };
                if stream.stream_type == "Subtitle" && stream.is_text_subtitle_stream {
                    let codec = subtitle_extension(stream.codec.as_deref());
                    let filename = format!("Stream.{codec}");
                    let mut subtitle = self.endpoint(&[
                        "Videos",
                        &item.id,
                        &source.id,
                        "Subtitles",
                        &index.to_string(),
                        &filename,
                    ])?;
                    subtitle
                        .query_pairs_mut()
                        .append_pair("api_key", &self.token);
                    subtitle_urls.push(subtitle.into());
                } else if stream.stream_type == "Audio" {
                    if let Some(mut delivery_url) = stream
                        .delivery_url
                        .as_deref()
                        .and_then(|value| self.same_server_url(value))
                    {
                        append_access_token(&mut delivery_url, &self.token);
                        audio_urls.push(delivery_url.into());
                        continue;
                    }
                    let Some(codec) = stream.codec.as_deref().and_then(safe_extension) else {
                        continue;
                    };
                    let filename = format!("stream.{codec}");
                    let mut audio = self.endpoint(&["Audio", &item.id, &filename])?;
                    audio
                        .query_pairs_mut()
                        .append_pair("MediaSourceId", &source.id)
                        .append_pair("AudioStreamIndex", &index.to_string())
                        .append_pair("api_key", &self.token);
                    audio_urls.push(audio.into());
                } else if stream.stream_type == "Subtitle"
                    && let Some(mut delivery_url) = stream
                        .delivery_url
                        .as_deref()
                        .and_then(|value| self.same_server_url(value))
                {
                    append_access_token(&mut delivery_url, &self.token);
                    subtitle_urls.push(delivery_url.into());
                }
            }
        }

        Ok(RemotePlaybackItem {
            item_id: item.id.clone(),
            media_source_id: source.map(|source| source.id.clone()),
            url: url.into(),
            title: playback_title(item),
            subtitle_urls,
            audio_urls,
            start_position_ticks: item
                .user_data
                .as_ref()
                .and_then(|data| data.playback_position_ticks)
                .unwrap_or_default()
                .max(0),
            run_time_ticks: item
                .run_time_ticks
                .or_else(|| source.and_then(|source| source.run_time_ticks))
                .filter(|ticks| *ticks > 0),
        })
    }

    fn same_server_url(&self, value: &str) -> Option<Url> {
        let url = Url::parse(value)
            .or_else(|_| self.api_base.join(value))
            .ok()?;
        if !matches!(url.scheme(), "http" | "https")
            || url.scheme() != self.api_base.scheme()
            || url.host_str() != self.api_base.host_str()
            || url.port_or_known_default() != self.api_base.port_or_known_default()
        {
            return None;
        }
        Some(url)
    }

    fn build(
        kind: &str,
        base_url: &str,
        token: &str,
        user_id: &str,
        device_id: &str,
    ) -> AppResult<Self> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("mpv-enjoy Home/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| remote_error("无法创建媒体服务器客户端", error))?;
        let mut api_base =
            Url::parse(base_url).map_err(|_| AppError::message("媒体服务器地址无效"))?;
        if kind == "emby"
            && !api_base
                .path_segments()
                .and_then(|mut segments| segments.rfind(|value| !value.is_empty()))
                .is_some_and(|value| value.eq_ignore_ascii_case("emby"))
        {
            api_base
                .path_segments_mut()
                .map_err(|_| AppError::message("媒体服务器地址不能作为 API 地址"))?
                .push("emby");
        }
        ensure_trailing_slash(&mut api_base);
        Ok(Self {
            client,
            api_base,
            kind: kind.to_string(),
            token: token.to_string(),
            user_id: user_id.to_string(),
            device_id: normalize_device_id(device_id)?,
        })
    }

    fn authenticate_by_name(
        &self,
        username: &str,
        password: &str,
    ) -> AppResult<AuthenticationResult> {
        let url = self.endpoint(&["Users", "AuthenticateByName"])?;
        let response = self
            .client
            .post(url)
            .headers(self.request_headers()?)
            .json(&AuthenticateRequest {
                username,
                pw: password,
            })
            .send()
            .map_err(|error| remote_error("无法连接媒体服务器", error))?;
        ensure_login_success(response)?
            .json()
            .map_err(|error| remote_error("登录响应格式无法识别", error))
    }

    fn get_json<T: DeserializeOwned>(
        &self,
        segments: &[&str],
        query: &[(&str, &str)],
    ) -> AppResult<T> {
        let response = ensure_success(self.send_get(segments, query)?)?;
        response
            .json()
            .map_err(|error| remote_error("媒体服务器返回了无法识别的数据", error))
    }

    fn send_get(&self, segments: &[&str], query: &[(&str, &str)]) -> AppResult<Response> {
        let mut url = self.endpoint(segments)?;
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query.iter().copied());
        }
        self.client
            .get(url)
            .headers(self.request_headers()?)
            .send()
            .map_err(|error| remote_error("无法连接媒体服务器", error))
    }

    fn post_json<T: Serialize>(&self, segments: &[&str], body: &T) -> AppResult<()> {
        let response = self
            .client
            .post(self.endpoint(segments)?)
            .headers(self.request_headers()?)
            .json(body)
            .send()
            .map_err(|error| remote_error("无法回传播放进度", error))?;
        ensure_success(response)?;
        Ok(())
    }

    fn request_headers(&self) -> AppResult<HeaderMap> {
        let mut headers = HeaderMap::new();
        let scheme = if self.kind == "emby" {
            "Emby"
        } else {
            "MediaBrowser"
        };
        let mut fields = vec![
            format!("Client=\"{CLIENT_NAME}\""),
            format!("Device=\"{DEVICE_NAME}\""),
            format!("DeviceId=\"{}\"", self.device_id),
            format!("Version=\"{}\"", env!("CARGO_PKG_VERSION")),
        ];
        if !self.user_id.is_empty() {
            fields.push(format!("UserId=\"{}\"", self.user_id));
        }
        if !self.token.is_empty() {
            fields.push(format!("Token=\"{}\"", self.token));
            headers.insert(
                TOKEN_HEADER,
                HeaderValue::from_str(&self.token)
                    .map_err(|_| AppError::message("媒体服务器令牌格式无效"))?,
            );
        }
        let authorization = HeaderValue::from_str(&format!("{scheme} {}", fields.join(", ")))
            .map_err(|_| AppError::message("媒体服务器认证请求格式无效"))?;
        headers.insert(AUTHORIZATION, authorization.clone());
        headers.insert(EMBY_AUTHORIZATION_HEADER, authorization);
        Ok(headers)
    }

    fn endpoint(&self, segments: &[&str]) -> AppResult<Url> {
        let mut url = self.api_base.clone();
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|_| AppError::message("媒体服务器地址不能作为 API 地址"))?;
            path.pop_if_empty();
            for segment in segments {
                path.push(segment);
            }
        }
        Ok(url)
    }
}

impl RemotePlaybackReporter {
    pub fn report(
        &self,
        kind: PlaybackReportKind,
        item: &RemotePlaybackItem,
        state: PlaybackState,
        playlist_index: usize,
        playlist_length: usize,
    ) -> AppResult<()> {
        let body = playback_check_in(
            &self.play_session_id,
            kind,
            item,
            state,
            playlist_index,
            playlist_length,
        );
        self.remote.post_json(playback_report_endpoint(kind), &body)
    }
}

fn playback_report_endpoint(kind: PlaybackReportKind) -> &'static [&'static str] {
    match kind {
        PlaybackReportKind::Started => &["Sessions", "Playing"],
        PlaybackReportKind::Progress => &["Sessions", "Playing", "Progress"],
        PlaybackReportKind::Stopped => &["Sessions", "Playing", "Stopped"],
    }
}

fn playback_check_in<'a>(
    play_session_id: &'a str,
    kind: PlaybackReportKind,
    item: &'a RemotePlaybackItem,
    state: PlaybackState,
    playlist_index: usize,
    playlist_length: usize,
) -> PlaybackCheckIn<'a> {
    PlaybackCheckIn {
        queueable_media_types: ["Video"],
        can_seek: true,
        item_id: &item.item_id,
        media_source_id: item.media_source_id.as_deref(),
        is_paused: state.paused,
        is_muted: state.muted,
        position_ticks: seconds_to_ticks(state.position_seconds).unwrap_or_default(),
        run_time_ticks: state
            .duration_seconds
            .and_then(seconds_to_ticks)
            .or(item.run_time_ticks),
        volume_level: state
            .volume
            .filter(|value| value.is_finite())
            .map(|value| value.round().clamp(0.0, 100.0) as i32),
        play_method: "DirectPlay",
        play_session_id,
        playlist_index,
        playlist_length,
        playback_rate: if state.playback_rate.is_finite() && state.playback_rate > 0.0 {
            state.playback_rate
        } else {
            1.0
        },
        event_name: (kind == PlaybackReportKind::Progress).then_some("TimeUpdate"),
    }
}

pub fn normalize_base_url(value: &str) -> AppResult<String> {
    let mut url = Url::parse(value.trim())
        .map_err(|_| AppError::message("服务器地址应为完整的 http:// 或 https:// 地址"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(AppError::message(
            "服务器地址只支持带主机名的 http:// 或 https:// 地址",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(AppError::message("服务器地址中不能包含用户名或密码"));
    }
    url.set_query(None);
    url.set_fragment(None);
    let trimmed = url.path().trim_end_matches('/').to_string();
    url.set_path(if trimmed.is_empty() { "/" } else { &trimmed });
    Ok(url.into())
}

fn normalize_kind(value: &str) -> AppResult<String> {
    let kind = value.trim().to_ascii_lowercase();
    if matches!(kind.as_str(), "emby" | "jellyfin") {
        Ok(kind)
    } else {
        Err(AppError::message("媒体服务器类型只支持 Emby 或 Jellyfin"))
    }
}

fn normalize_username(value: &str) -> AppResult<String> {
    let username = value.trim();
    if username.is_empty() || username.len() > 256 || username.chars().any(char::is_control) {
        return Err(AppError::message("媒体服务器用户名不能为空或过长"));
    }
    Ok(username.to_string())
}

fn normalize_password(value: &str) -> AppResult<String> {
    if value.len() > 4096 {
        return Err(AppError::message("媒体服务器密码过长"));
    }
    Ok(value.to_string())
}

fn normalize_token(value: &str) -> AppResult<String> {
    let token = value.trim();
    if token.is_empty() || token.len() > 4096 {
        return Err(AppError::message("媒体服务器令牌不能为空或过长"));
    }
    if !token
        .bytes()
        .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'-' | b'_' | b'.' | b'~'))
    {
        return Err(AppError::message("媒体服务器令牌包含不安全字符"));
    }
    Ok(token.to_string())
}

fn normalize_device_id(value: &str) -> AppResult<String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(AppError::message("媒体服务器设备标识无效"));
    }
    Ok(value.to_string())
}

fn safe_extension(value: &str) -> Option<&str> {
    let value = value.trim();
    if !value.is_empty()
        && value.len() <= 16
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        Some(value)
    } else {
        None
    }
}

fn subtitle_extension(codec: Option<&str>) -> &'static str {
    match codec.map(str::to_ascii_lowercase).as_deref() {
        Some("ass") => "ass",
        Some("ssa") => "ssa",
        Some("vtt" | "webvtt") => "vtt",
        _ => "srt",
    }
}

fn append_access_token(url: &mut Url, token: &str) {
    let already_authenticated = url.query_pairs().any(|(key, _)| {
        key.eq_ignore_ascii_case("api_key") || key.eq_ignore_ascii_case(TOKEN_HEADER)
    });
    if !already_authenticated {
        url.query_pairs_mut().append_pair("api_key", token);
    }
}

fn new_play_session_id() -> String {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("mpvenjoy{nonce:x}")
}

fn seconds_to_ticks(seconds: f64) -> Option<i64> {
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    let ticks = seconds * 10_000_000.0;
    Some(ticks.min(i64::MAX as f64).round() as i64)
}

fn playback_title(item: &BaseItem) -> String {
    if item.item_type == "Episode" {
        let series = item.series_name.as_deref().unwrap_or("剧集");
        match (item.parent_index_number, item.index_number) {
            (Some(season), Some(episode)) => {
                format!("{series} S{season:02}E{episode:02} - {}", item.name)
            }
            _ => format!("{series} - {}", item.name),
        }
    } else {
        item.name.clone()
    }
}

fn remote_recent_media(item: BaseItem) -> Option<RemoteRecentMedia> {
    let updated_at = item
        .date_created
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.timestamp())
        .unwrap_or_default();
    let (target_id, target_name, context) = if item.item_type == "Episode" {
        let target_id = item.series_id.clone()?;
        let target_name = item
            .series_name
            .clone()
            .unwrap_or_else(|| item.name.clone());
        let episode = match (item.parent_index_number, item.index_number) {
            (Some(season), Some(episode)) => format!("S{season:02}E{episode:02}"),
            (_, Some(episode)) => format!("E{episode:02}"),
            _ => "单集".to_string(),
        };
        (
            target_id,
            target_name.clone(),
            format!("{target_name} · {episode}"),
        )
    } else {
        let context = if item.item_type == "Movie" {
            "电影"
        } else {
            "视频"
        };
        (item.id.clone(), item.name.clone(), context.to_string())
    };

    Some(RemoteRecentMedia {
        item_id: item.id,
        target_id,
        target_name,
        name: item.name,
        context,
        item_type: item.item_type,
        updated_at,
    })
}

fn sort_remote_items(items: &mut [BaseItem]) {
    items.sort_by(compare_remote_items);
}

fn compare_remote_items(left: &BaseItem, right: &BaseItem) -> Ordering {
    let numeric_order = match (left.item_type.as_str(), right.item_type.as_str()) {
        ("Episode", "Episode") => {
            compare_optional_index(left.parent_index_number, right.parent_index_number)
                .then_with(|| compare_optional_index(left.index_number, right.index_number))
        }
        ("Season", "Season") => compare_optional_index(left.index_number, right.index_number),
        _ => Ordering::Equal,
    };
    numeric_order.then_with(|| natural_cmp(&left.name, &right.name))
}

fn compare_optional_index(left: Option<i32>, right: Option<i32>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn validate_identifier(value: &str, label: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 256
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '"' | '\\'))
    {
        return Err(AppError::message(format!("{label}标识无效")));
    }
    Ok(())
}

fn ensure_trailing_slash(url: &mut Url) {
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
}

fn ensure_success(response: Response) -> AppResult<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    Err(remote_status_error(status.as_u16()))
}

fn remote_status_error(status: u16) -> AppError {
    let message = match status {
        401 => {
            return authentication_required("媒体服务器登录已失效，请重新登录");
        }
        403 => "当前用户没有访问此内容的权限；如果账号权限未改变，请尝试重新登录".to_string(),
        404 => "媒体服务器没有提供所需的兼容接口".to_string(),
        value => format!("媒体服务器请求失败（HTTP {value}）"),
    };
    AppError::message(message)
}

pub(crate) fn requires_authentication(error: &AppError) -> bool {
    error.0.starts_with(AUTHENTICATION_REQUIRED_MARKER)
}

pub(crate) fn authentication_required(message: impl AsRef<str>) -> AppError {
    AppError::message(format!(
        "{AUTHENTICATION_REQUIRED_MARKER}{}",
        message.as_ref()
    ))
}

fn ensure_login_success(response: Response) -> AppResult<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let message = match status.as_u16() {
        400 | 401 | 403 | 500 => {
            format!("服务器未接受用户名或密码（HTTP {}）", status.as_u16())
        }
        404 => "服务器没有提供用户名密码登录接口".to_string(),
        value => format!("媒体服务器登录请求失败（HTTP {value}）"),
    };
    Err(AppError::message(message))
}

fn contextual_error(context: &str, error: AppError) -> AppError {
    AppError::message(format!("{context}：{}", error.0))
}

fn library_include_types(parent: &BaseItem) -> &'static str {
    match parent.collection_type.as_deref() {
        Some("movies") => "Movie",
        Some("tvshows") => "Series",
        Some("homevideos" | "musicvideos") => "Video,Movie",
        Some("music" | "games" | "books" | "livetv" | "channels") => "",
        _ if parent.item_type == "BoxSet" => "Movie,Series,Video",
        _ => "Movie,Series,Video",
    }
}

fn item_primary_image_id(item: &BaseItem) -> Option<String> {
    item.image_tags
        .contains_key("Primary")
        .then(|| item.id.clone())
}

fn item_backdrop_image_id(item: &BaseItem) -> Option<String> {
    if !item.backdrop_image_tags.is_empty() {
        Some(item.id.clone())
    } else if !item.parent_backdrop_image_tags.is_empty() {
        item.parent_backdrop_item_id.clone()
    } else {
        None
    }
}

fn user_playback(item: &BaseItem) -> (bool, i64, Option<f64>) {
    let user_data = item.user_data.as_ref();
    (
        user_data.is_some_and(|data| data.played),
        user_data
            .and_then(|data| data.playback_position_ticks)
            .unwrap_or_default()
            .max(0),
        user_data
            .and_then(|data| data.played_percentage)
            .filter(|percentage| percentage.is_finite())
            .map(|percentage| percentage.clamp(0.0, 100.0)),
    )
}

fn season_detail(season: &BaseItem, episodes: &[BaseItem]) -> RemoteSeasonDetail {
    let matching_episodes = episodes
        .iter()
        .filter(|episode| {
            episode.season_id.as_deref() == Some(season.id.as_str())
                || episode.parent_id.as_deref() == Some(season.id.as_str())
                || (season.index_number.is_some()
                    && episode.parent_index_number == season.index_number)
        })
        .count();
    RemoteSeasonDetail {
        id: season.id.clone(),
        name: season.name.clone(),
        index_number: season.index_number,
        overview: season
            .overview
            .clone()
            .filter(|value| !value.trim().is_empty()),
        episode_count: matching_episodes.max(season.child_count.unwrap_or_default()),
        unplayed_count: season
            .user_data
            .as_ref()
            .and_then(|data| data.unplayed_item_count),
        played: season.user_data.as_ref().is_some_and(|data| data.played),
        primary_image_id: item_primary_image_id(season),
    }
}

fn episode_detail(item: &BaseItem) -> RemoteEpisodeDetail {
    let (played, playback_position_ticks, played_percentage) = user_playback(item);
    RemoteEpisodeDetail {
        id: item.id.clone(),
        name: item.name.clone(),
        overview: item
            .overview
            .clone()
            .filter(|value| !value.trim().is_empty()),
        index_number: item.index_number,
        parent_index_number: item.parent_index_number,
        season_id: item.season_id.clone().or_else(|| item.parent_id.clone()),
        premiere_date: item.premiere_date.clone(),
        runtime_ticks: item.run_time_ticks,
        played,
        playback_position_ticks,
        played_percentage,
        primary_image_id: item_primary_image_id(item),
    }
}

fn media_detail(
    item: BaseItem,
    seasons: Vec<RemoteSeasonDetail>,
    episodes: Vec<RemoteEpisodeDetail>,
) -> RemoteMediaDetail {
    let (played, playback_position_ticks, played_percentage) = user_playback(&item);
    let primary_image_id = item_primary_image_id(&item);
    let backdrop_image_id = item_backdrop_image_id(&item);
    RemoteMediaDetail {
        id: item.id,
        name: item.name,
        item_type: item.item_type,
        overview: item.overview.filter(|value| !value.trim().is_empty()),
        tagline: item
            .taglines
            .into_iter()
            .find(|value| !value.trim().is_empty()),
        genres: item.genres,
        production_year: item.production_year,
        premiere_date: item.premiere_date,
        runtime_ticks: item.run_time_ticks,
        community_rating: item.community_rating,
        official_rating: item.official_rating,
        played,
        playback_position_ticks,
        played_percentage,
        primary_image_id,
        backdrop_image_id,
        seasons,
        episodes,
        people: item
            .people
            .into_iter()
            .map(|person| RemotePersonDetail {
                primary_image_id: person.primary_image_tag.as_ref().and(person.id.clone()),
                id: person.id,
                name: person.name,
                role: person.role,
                person_type: person.person_type,
            })
            .collect(),
    }
}

fn is_video_or_container(item: &BaseItem) -> bool {
    if matches!(
        item.collection_type.as_deref(),
        Some("music" | "games" | "books" | "livetv" | "channels")
    ) {
        return false;
    }
    item.is_folder
        || item.media_type.as_deref() == Some("Video")
        || matches!(
            item.item_type.as_str(),
            "Folder"
                | "CollectionFolder"
                | "Series"
                | "Season"
                | "BoxSet"
                | "Movie"
                | "Episode"
                | "Video"
        )
}

fn to_library_entry(item: BaseItem) -> RemoteLibraryEntry {
    let is_container = matches!(
        item.item_type.as_str(),
        "Folder" | "CollectionFolder" | "BoxSet"
    );
    let subtitle = if item.item_type == "Episode" {
        match (item.parent_index_number, item.index_number) {
            (Some(season), Some(episode)) => Some(format!(
                "{} · S{season:02}E{episode:02}",
                item.series_name.as_deref().unwrap_or("剧集")
            )),
            _ => item.series_name.clone(),
        }
    } else {
        item.production_year
            .map(|year| year.to_string())
            .or_else(|| {
                item.collection_type
                    .as_deref()
                    .map(collection_type_label)
                    .map(str::to_string)
            })
    };
    RemoteLibraryEntry {
        id: item.id,
        name: item.name,
        kind: if is_container { "collection" } else { "detail" }.to_string(),
        item_type: item.item_type,
        subtitle,
        child_count: item.child_count.or(item.recursive_item_count).unwrap_or(0),
        has_image: item.image_tags.contains_key("Primary"),
        image_aspect_ratio: item.primary_image_aspect_ratio,
        index_number: item.index_number,
        parent_index_number: item.parent_index_number,
    }
}

fn collection_type_label(value: &str) -> &'static str {
    match value {
        "movies" => "电影",
        "tvshows" => "剧集",
        "homevideos" => "家庭视频",
        "musicvideos" => "音乐视频",
        _ => "媒体库",
    }
}

fn kind_label(kind: &str) -> &'static str {
    if kind == "emby" { "Emby" } else { "Jellyfin" }
}

fn remote_error(context: &str, error: impl std::fmt::Display) -> AppError {
    AppError::message(format!("{context}：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn normalizes_server_urls_without_credentials_or_query() {
        assert_eq!(
            normalize_base_url(" https://media.example/jellyfin/?key=secret ").unwrap(),
            "https://media.example/jellyfin"
        );
        assert!(normalize_base_url("ftp://media.example").is_err());
        assert!(normalize_base_url("https://user:pass@media.example").is_err());
    }

    #[test]
    fn rejects_header_injection_in_token() {
        assert!(normalize_token("valid-token_123").is_ok());
        assert!(normalize_token("bad\r\nHeader: value").is_err());
    }

    #[test]
    fn preserves_base_urls_and_adds_only_the_emby_api_prefix() {
        let emby = RemoteClient::build(
            "emby",
            "https://media.example/base",
            "token",
            "user",
            "test-device",
        )
        .unwrap();
        assert_eq!(
            emby.endpoint(&["Users", "user", "Views"]).unwrap().as_str(),
            "https://media.example/base/emby/Users/user/Views"
        );

        let jellyfin = RemoteClient::build(
            "jellyfin",
            "https://media.example/jellyfin",
            "token",
            "user",
            "test-device",
        )
        .unwrap();
        assert_eq!(
            jellyfin
                .endpoint(&["Users", "user", "Views"])
                .unwrap()
                .as_str(),
            "https://media.example/jellyfin/Users/user/Views"
        );
    }

    #[test]
    fn builds_compatible_authorization_headers() {
        let emby = RemoteClient::build(
            "emby",
            "https://media.example",
            "valid-token",
            "user-id",
            "test-device",
        )
        .unwrap();
        let headers = emby.request_headers().unwrap();
        assert_eq!(
            headers.get(TOKEN_HEADER).unwrap().to_str().unwrap(),
            "valid-token"
        );
        let authorization = headers.get(AUTHORIZATION).unwrap().to_str().unwrap();
        assert!(authorization.starts_with("Emby "));
        assert!(authorization.contains("Client=\"mpv-enjoy Home\""));
        assert!(authorization.contains("UserId=\"user-id\""));
        assert!(authorization.contains("Token=\"valid-token\""));
        assert!(authorization.contains("DeviceId=\"test-device\""));
        assert_eq!(
            headers
                .get(EMBY_AUTHORIZATION_HEADER)
                .unwrap()
                .to_str()
                .unwrap(),
            authorization
        );

        let jellyfin =
            RemoteClient::build("jellyfin", "https://media.example", "", "", "test-device")
                .unwrap();
        let headers = jellyfin.request_headers().unwrap();
        assert!(
            headers
                .get(AUTHORIZATION)
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("MediaBrowser ")
        );
        assert!(headers.get(TOKEN_HEADER).is_none());
    }

    #[test]
    fn validates_login_fields_without_requiring_a_password() {
        assert_eq!(normalize_username(" media-user ").unwrap(), "media-user");
        assert_eq!(normalize_password("").unwrap(), "");
        assert_eq!(normalize_device_id("install-123").unwrap(), "install-123");
        assert!(normalize_username("\n").is_err());
        assert!(normalize_device_id("bad\"device").is_err());
        assert!(validate_identifier("bad\"user", "用户 ID").is_err());
    }

    #[test]
    fn distinguishes_revoked_tokens_from_permission_failures() {
        let unauthorized = remote_status_error(401);
        assert!(unauthorized.0.starts_with(AUTHENTICATION_REQUIRED_MARKER));
        assert!(unauthorized.0.contains("重新登录"));

        let forbidden = remote_status_error(403);
        assert!(!forbidden.0.starts_with(AUTHENTICATION_REQUIRED_MARKER));
        assert!(forbidden.0.contains("权限"));
    }

    #[test]
    fn maps_recent_episode_to_its_series_detail() {
        let recent = remote_recent_media(BaseItem {
            id: "episode-id".to_string(),
            name: "新的单集".to_string(),
            item_type: "Episode".to_string(),
            series_id: Some("series-id".to_string()),
            series_name: Some("示例剧集".to_string()),
            parent_index_number: Some(2),
            index_number: Some(3),
            date_created: Some("2026-08-06T12:30:00Z".to_string()),
            ..BaseItem::default()
        })
        .unwrap();

        assert_eq!(recent.item_id, "episode-id");
        assert_eq!(recent.target_id, "series-id");
        assert_eq!(recent.target_name, "示例剧集");
        assert_eq!(recent.context, "示例剧集 · S02E03");
        assert_eq!(recent.updated_at, 1_786_019_400);
    }

    #[test]
    fn builds_direct_play_and_external_track_urls() {
        let remote = RemoteClient::build(
            "emby",
            "https://media.example",
            "valid-token",
            "user-id",
            "test-device",
        )
        .unwrap();
        let item = BaseItem {
            id: "episode-id".to_string(),
            name: "Episode".to_string(),
            item_type: "Episode".to_string(),
            is_folder: false,
            media_type: Some("Video".to_string()),
            collection_type: None,
            child_count: None,
            recursive_item_count: None,
            index_number: Some(2),
            parent_index_number: Some(1),
            series_name: Some("Series".to_string()),
            series_id: Some("series-id".to_string()),
            production_year: None,
            run_time_ticks: Some(1_400_000_000),
            primary_image_aspect_ratio: Some(16.0 / 9.0),
            user_data: Some(UserItemData {
                playback_position_ticks: Some(123_450_000),
                ..Default::default()
            }),
            media_sources: vec![MediaSource {
                id: "source-id".to_string(),
                container: Some("mkv".to_string()),
                run_time_ticks: Some(1_400_000_000),
                media_streams: vec![
                    MediaStream {
                        stream_type: "Subtitle".to_string(),
                        index: Some(3),
                        codec: Some("ass".to_string()),
                        is_external: true,
                        is_text_subtitle_stream: true,
                        delivery_url: None,
                    },
                    MediaStream {
                        stream_type: "Audio".to_string(),
                        index: Some(4),
                        codec: Some("aac".to_string()),
                        is_external: true,
                        is_text_subtitle_stream: false,
                        delivery_url: None,
                    },
                ],
            }],
            image_tags: HashMap::new(),
            ..Default::default()
        };

        let playback = remote.playback_item(&item, "session123").unwrap();
        assert_eq!(
            playback.url,
            "https://media.example/emby/Videos/episode-id/stream.mkv?DeviceId=test-device&PlaySessionId=session123&api_key=valid-token&Static=true&MediaSourceId=source-id"
        );
        assert_eq!(playback.title, "Series S01E02 - Episode");
        assert_eq!(playback.start_position_ticks, 123_450_000);
        assert_eq!(playback.run_time_ticks, Some(1_400_000_000));
        assert_eq!(
            playback.subtitle_urls,
            vec![
                "https://media.example/emby/Videos/episode-id/source-id/Subtitles/3/Stream.ass?api_key=valid-token"
            ]
        );
        assert_eq!(
            playback.audio_urls,
            vec![
                "https://media.example/emby/Audio/episode-id/stream.aac?MediaSourceId=source-id&AudioStreamIndex=4&api_key=valid-token"
            ]
        );
    }

    #[test]
    fn sorts_episodes_by_season_and_episode_before_title() {
        let mut items = vec![
            BaseItem {
                name: "标题排在最前但属于第十一集".to_string(),
                item_type: "Episode".to_string(),
                index_number: Some(11),
                parent_index_number: Some(1),
                ..Default::default()
            },
            BaseItem {
                name: "标题排在最后但属于第二集".to_string(),
                item_type: "Episode".to_string(),
                index_number: Some(2),
                parent_index_number: Some(1),
                ..Default::default()
            },
            BaseItem {
                name: "第二季第一集".to_string(),
                item_type: "Episode".to_string(),
                index_number: Some(1),
                parent_index_number: Some(2),
                ..Default::default()
            },
        ];

        sort_remote_items(&mut items);
        assert_eq!(
            items
                .iter()
                .map(|item| (item.parent_index_number, item.index_number))
                .collect::<Vec<_>>(),
            vec![(Some(1), Some(2)), (Some(1), Some(11)), (Some(2), Some(1))]
        );
    }

    #[test]
    fn library_views_query_metadata_items_instead_of_physical_folders() {
        let movie_library = BaseItem {
            item_type: "CollectionFolder".to_string(),
            collection_type: Some("movies".to_string()),
            ..Default::default()
        };
        let series_library = BaseItem {
            item_type: "CollectionFolder".to_string(),
            collection_type: Some("tvshows".to_string()),
            ..Default::default()
        };
        assert_eq!(library_include_types(&movie_library), "Movie");
        assert_eq!(library_include_types(&series_library), "Series");

        let series = to_library_entry(BaseItem {
            id: "series-id".to_string(),
            name: "元数据剧名".to_string(),
            item_type: "Series".to_string(),
            is_folder: true,
            media_type: None,
            ..Default::default()
        });
        assert_eq!(series.kind, "detail");
        assert_eq!(series.name, "元数据剧名");
    }

    #[test]
    fn keeps_season_metadata_while_counting_its_episodes() {
        let season = BaseItem {
            id: "season-id".to_string(),
            name: "第一季".to_string(),
            item_type: "Season".to_string(),
            index_number: Some(1),
            overview: Some("季度简介".to_string()),
            image_tags: HashMap::from([("Primary".to_string(), "tag".to_string())]),
            ..Default::default()
        };
        let episodes = vec![
            BaseItem {
                season_id: Some("season-id".to_string()),
                parent_index_number: Some(1),
                ..Default::default()
            },
            BaseItem {
                parent_id: Some("season-id".to_string()),
                parent_index_number: Some(1),
                ..Default::default()
            },
        ];
        let detail = season_detail(&season, &episodes);
        assert_eq!(detail.overview.as_deref(), Some("季度简介"));
        assert_eq!(detail.episode_count, 2);
        assert_eq!(detail.primary_image_id.as_deref(), Some("season-id"));
    }

    #[test]
    fn reports_playback_progress_with_emby_compatible_fields() {
        let item = RemotePlaybackItem {
            item_id: "episode-id".to_string(),
            media_source_id: Some("source-id".to_string()),
            url: "https://media.example/video".to_string(),
            title: "Series S01E02 - Episode".to_string(),
            subtitle_urls: Vec::new(),
            audio_urls: Vec::new(),
            start_position_ticks: 0,
            run_time_ticks: Some(1_400_000_000),
        };
        let body = playback_check_in(
            "session-id",
            PlaybackReportKind::Progress,
            &item,
            PlaybackState {
                position_seconds: 12.345,
                duration_seconds: Some(140.0),
                paused: false,
                muted: false,
                volume: Some(72.0),
                playback_rate: 1.25,
            },
            1,
            12,
        );
        let body: Value = serde_json::to_value(body).unwrap();
        assert_eq!(
            playback_report_endpoint(PlaybackReportKind::Progress),
            &["Sessions", "Playing", "Progress"]
        );
        assert_eq!(body["ItemId"], "episode-id");
        assert_eq!(body["MediaSourceId"], "source-id");
        assert_eq!(body["PositionTicks"], 123_450_000);
        assert_eq!(body["RunTimeTicks"], 1_400_000_000);
        assert_eq!(body["PlaySessionId"], "session-id");
        assert_eq!(body["PlaylistIndex"], 1);
        assert_eq!(body["PlaylistLength"], 12);
        assert_eq!(body["EventName"], "TimeUpdate");
    }
}
