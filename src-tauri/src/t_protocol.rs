use tauri::{Builder, Wry};
use std::io::Read;

use crate::{t_image, t_sqlite};
use crate::t_raw_display::{RawDisplayOptions, RawPreviewMode};

fn text_response(status: http::StatusCode, body: &str) -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(status)
        .header(http::header::CONTENT_TYPE, "text/plain")
        .header(http::header::CACHE_CONTROL, "no-store")
        .header(http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(http::header::ACCESS_CONTROL_ALLOW_METHODS, "GET, OPTIONS")
        .header(http::header::ACCESS_CONTROL_ALLOW_HEADERS, "*")
        .body(body.as_bytes().to_vec())
        .unwrap()
}

fn detect_image_mime(data: &[u8]) -> &'static str {
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        "image/png"
    } else if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if data.starts_with(&[0x47, 0x49, 0x46, 0x38]) {
        "image/gif"
    } else if data.starts_with(&[0x49, 0x49, 0x2A, 0x00])
        || data.starts_with(&[0x4D, 0x4D, 0x00, 0x2A])
        || data.starts_with(&[0x4D, 0x4D, 0x00, 0x2B])
        || data.starts_with(&[0x49, 0x49, 0x2B, 0x00])
    {
        "image/tiff"
    } else if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP") {
        "image/webp"
    } else {
        detect_isobmff_mime(data)
    }
}

fn detect_isobmff_mime(data: &[u8]) -> &'static str {
    if data.len() < 16 || &data[4..8] != b"ftyp" {
        return "application/octet-stream";
    }

    let box_size = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
    if box_size < 16 || box_size > data.len() {
        return "application/octet-stream";
    }

    match &data[8..12] {
        b"avif" | b"avis" => "image/avif",
        b"heic" | b"heix" | b"hevc" | b"hevx" => "image/heic",
        b"mif1" | b"heif" => "image/heif",
        _ => {
            for brand in data[16..box_size].chunks(4) {
                if brand.len() < 4 {
                    break;
                }
                match brand {
                    b"avif" | b"avis" => return "image/avif",
                    b"heic" | b"heix" | b"hevc" | b"hevx" => {
                        return "image/heic";
                    }
                    b"mif1" | b"heif" => return "image/heif",
                    _ => {}
                }
            }
            "application/octet-stream"
        }
    }
}

fn image_response(data: Vec<u8>) -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(http::StatusCode::OK)
        .header(http::header::CONTENT_TYPE, detect_image_mime(&data))
        .header(http::header::CACHE_CONTROL, "max-age=31536000, immutable")
        .header(http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(http::header::ACCESS_CONTROL_ALLOW_METHODS, "GET, OPTIONS")
        .header(http::header::ACCESS_CONTROL_ALLOW_HEADERS, "*")
        .body(data)
        .unwrap()
}

fn raw_display_options(query: Option<&str>) -> RawDisplayOptions {
    let mut options = RawDisplayOptions::default();
    for pair in query.unwrap_or_default().split('&') {
        match pair {
            "rawPreviewMode=embedded" => options.mode = RawPreviewMode::Embedded,
            "rawPreviewMode=rendered" => options.mode = RawPreviewMode::Rendered,
            "rawAutoBright=true" => options.auto_bright = true,
            "rawPairDisplay=jpeg" => options.prefer_pair = true,
            _ => {}
        }
    }
    options
}

fn raw_image_response(data: Vec<u8>) -> http::Response<Vec<u8>> {
    let mut response = image_response(data);
    // Revalidate external companion changes even when the RAW itself is unchanged.
    response.headers_mut().insert(http::header::CACHE_CONTROL, http::HeaderValue::from_static("no-cache"));
    response
}

fn raw_preview_response(data: Vec<u8>, source: &'static str, unavailable: bool, pair: &'static str) -> http::Response<Vec<u8>> {
    let mut response = raw_image_response(data);
    let headers = response.headers_mut();
    headers.insert("X-Raw-Source", http::HeaderValue::from_static(source));
    headers.insert("X-Raw-Embedded-Unavailable", http::HeaderValue::from_static(if unavailable { "true" } else { "false" }));
    headers.insert("X-Raw-Pair", http::HeaderValue::from_static(pair));
    headers.insert(http::header::ACCESS_CONTROL_EXPOSE_HEADERS, http::HeaderValue::from_static("X-Raw-Source, X-Raw-Embedded-Unavailable, X-Raw-Pair"));
    response
}

#[cfg(test)]
mod raw_display_tests {
    use super::*;

    #[test]
    fn raw_display_protocol_parses_modes_and_pair_policy() {
        let options = raw_display_options(Some("v=42&rawPreviewMode=rendered&rawPairDisplay=jpeg"));
        assert_eq!(options.mode, RawPreviewMode::Rendered);
        assert!(options.prefer_pair);
        assert_eq!(raw_display_options(Some("rawPreviewMode=embedded")).mode, RawPreviewMode::Embedded);
        assert!(raw_display_options(Some("rawPreviewMode=embedded&rawAutoBright=true")).auto_bright);
        assert!(!raw_display_options(None).prefer_pair);
    }
}

fn thumbnail_error_response(
    status: http::StatusCode,
    file_id: i64,
    reason: &str,
) -> http::Response<Vec<u8>> {
    text_response(status, &format!("Thumbnail for file {file_id}: {reason}"))
}

fn load_protocol_thumbnail(
    file_id: i64,
    library_id: &str,
    thumbnail_size: u32,
    options: RawDisplayOptions,
) -> Result<http::Response<Vec<u8>>, String> {
    // Keep both the database and cache namespace stable, including after queueing.
    crate::t_cmds::with_current_library(library_id, || {
        if crate::t_media_lifecycle::is_private_file(file_id)? {
            return Ok(text_response(http::StatusCode::FORBIDDEN,"Use the private viewer"));
        }
        let Some(file) = t_sqlite::AFile::get_file_info(file_id)? else {
            return Ok(thumbnail_error_response(
                http::StatusCode::NOT_FOUND,
                file_id,
                "file is not in this library",
            ));
        };
        let Some(path) = file.file_path.as_deref().filter(|path| !path.is_empty()) else {
            return Ok(thumbnail_error_response(
                http::StatusCode::NOT_FOUND,
                file_id,
                "source path is missing",
            ));
        };
        // Reuse the existing cache validation, deduplication and decoder path for all types.
        // A cache miss is pending work, not a 404; wait for the requested size to be generated.
        let thumbnail = t_sqlite::AThumb::get_or_create_thumb(
            file_id,
            path,
            file.file_type.unwrap_or(0),
            file.e_orientation.unwrap_or(1) as i32,
            thumbnail_size,
            options,
            false,
            file.duration.map(|duration| duration.max(0) as u64),
            None,
        )?;
        let data = match thumbnail {
            Some(thumb) if thumb.error_code == 0 => thumb.thumb_data,
            Some(thumb) if thumb.error_code == 2 => {
                if crate::t_utils::is_cloud_only_path(path) {
                    return Ok(thumbnail_error_response(
                        http::StatusCode::NOT_FOUND, file_id,
                        "original is online-only and no local thumbnail is cached",
                    ));
                }
                // Small images deliberately use the original instead of a separate cache file.
                const MAX_ORIGINAL_THUMB_BYTES: u64 = 32 * 1024 * 1024;
                let mut input = std::fs::File::open(path)
                    .map_err(|error| format!("cannot open original: {error}"))?;
                let mut bytes = Vec::new();
                std::io::Read::take(&mut input, MAX_ORIGINAL_THUMB_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|error| error.to_string())?;
                if bytes.len() as u64 > MAX_ORIGINAL_THUMB_BYTES {
                    return Ok(thumbnail_error_response(
                        http::StatusCode::PAYLOAD_TOO_LARGE,
                        file_id,
                        "original exceeds thumbnail response size limit",
                    ));
                }
                Some(bytes)
            }
            _ => None,
        };
        Ok(match data.filter(|bytes| !bytes.is_empty()) {
            Some(data) if file.file_type == Some(3) => raw_image_response(data),
            Some(data) => image_response(data),
            None if !std::path::Path::new(path).is_file() => thumbnail_error_response(
                http::StatusCode::NOT_FOUND,
                file_id,
                "original is unavailable and no cached thumbnail exists",
            ),
            None => thumbnail_error_response(
                http::StatusCode::UNPROCESSABLE_ENTITY,
                file_id,
                "could not decode or generate thumbnail; check decoder support and source integrity",
            ),
        })
    })
}

fn thumbnail_request_failure(file_id: i64, reason: &str) -> http::Response<Vec<u8>> {
    let status = if reason == "Library changed" {
        http::StatusCode::CONFLICT
    } else {
        http::StatusCode::INTERNAL_SERVER_ERROR
    };
    thumbnail_error_response(status, file_id, reason)
}

pub fn register_protocols(builder: Builder<Wry>) -> Builder<Wry> {
    builder
        .register_asynchronous_uri_scheme_protocol("thumb", |_ctx, request, responder| {
            // URL format: thumb://localhost/{library_id}/{file_id}
            // library_id is also used for cache namespace isolation.
            let path = request.uri().path();
            let mut segments = path.trim_start_matches('/').split('/');
            let library_id = segments.next().unwrap_or("default").to_string();
            let file_id_str = segments.next().unwrap_or("");
            let file_id: i64 = file_id_str.parse().unwrap_or(0);

            if library_id.is_empty() || file_id <= 0 {
                responder.respond(text_response(
                    http::StatusCode::BAD_REQUEST,
                    "invalid file_id",
                ));
                return;
            }

            let options = raw_display_options(request.uri().query());
            let thumbnail_size = request
                .uri()
                .query()
                .unwrap_or_default()
                .split('&')
                .find_map(|part| {
                    part.strip_prefix("size=")
                        .and_then(|value| value.parse::<u32>().ok())
                })
                .unwrap_or(512)
                .clamp(64, 2048);
            tauri::async_runtime::spawn(async move {
                let permit = match t_sqlite::thumb_background_generation_permits()
                    .acquire_owned()
                    .await
                {
                    Ok(permit) => permit,
                    Err(_) => {
                        responder.respond(thumbnail_error_response(
                            http::StatusCode::SERVICE_UNAVAILABLE,
                            file_id,
                            "thumbnail queue closed",
                        ));
                        return;
                    }
                };
                // All file/SQLite work belongs on blocking workers, never the UI or async reactor.
                let result = tauri::async_runtime::spawn_blocking(move || {
                    let _permit = permit;
                    load_protocol_thumbnail(file_id, &library_id, thumbnail_size, options)
                })
                .await;
                responder.respond(match result {
                    Ok(Ok(response)) => response,
                    Ok(Err(error)) => thumbnail_request_failure(file_id, &error),
                    Err(error) => thumbnail_request_failure(
                        file_id,
                        &format!("thumbnail worker failed: {error}"),
                    ),
                });
            });
        })
        .register_asynchronous_uri_scheme_protocol("preview", |_ctx, request, responder| {
            // URL format: preview://localhost/{library_id}/{file_id}?rawPreviewMode=embedded
            // library_id isolates browser caches and guards stale requests after a library switch
            let path = request.uri().path();
            let file_id_str = path.rsplit('/').next().unwrap_or("");
            let file_id: i64 = file_id_str.parse().unwrap_or(0);
            let options = raw_display_options(request.uri().query());
            let cached_only = request.uri().query().unwrap_or_default().split('&')
                .any(|param| param == "cachedOnly=true");
            let for_editing = request.uri().query().unwrap_or_default().split('&')
                .any(|param| param == "forEditing=true");
            let raw_request = request.uri().query().unwrap_or_default().split('&')
                .find_map(|param| param.strip_prefix("rawViewerId="))
                .map(t_image::register_raw_preview_request);

            if file_id <= 0 {
                responder.respond(text_response(
                    http::StatusCode::BAD_REQUEST,
                    "invalid file_id",
                ));
                return;
            }

            let library_id = path.trim_start_matches('/').split('/').next().unwrap_or("default").to_string();
            tauri::async_runtime::spawn(async move {
                // Scheme callbacks can run on the UI thread. Never wait for a
                // library lock, configuration IO or SQLite there.
                let lookup = tauri::async_runtime::spawn_blocking(move || {
                    crate::t_cmds::with_current_library(&library_id, || {
                        if crate::t_media_lifecycle::is_private_file(file_id)? {return Err("Use the private viewer".into());}
                        let file = t_sqlite::AFile::get_file_info(file_id)?
                            .ok_or_else(|| "File not found".to_string())?;
                        let companion = if !options.prefer_pair {
                            None
                        } else if cached_only && file.media_subtype.as_deref() == Some("raw_jpeg_pair") {
                            file.live_photo_video_id.and_then(|id| t_sqlite::AFile::get_file_info(id).ok().flatten())
                        } else {
                            crate::t_raw_display::paired_file(file_id, options)
                        };
                        if let Some(pair)=&companion {if crate::t_media_lifecycle::is_private_file(pair.id.ok_or("Invalid companion ID")?)? {return Err("Private companion".into());}}
                        Ok((file, companion))
                    })
                }).await;
                let (file, companion) = match lookup {
                    Ok(Ok(files)) => files,
                    _ => {
                        responder.respond(text_response(http::StatusCode::NOT_FOUND, "file not found"));
                        return;
                    }
                };
                let is_raw = file.file_type == Some(3);
                let file_path = match file.file_path {
                    Some(path) if !path.is_empty() => path,
                    _ => {
                        responder.respond(text_response(http::StatusCode::NOT_FOUND, "file path not found"));
                        return;
                    }
                };
                if cached_only {
                    let paired_cache = options.prefer_pair.then(|| companion.as_ref()
                        .and_then(|file| file.file_path.as_deref())
                        .and_then(|path| t_image::get_cached_preview_bytes(path, options))).flatten();
                    let data = paired_cache.or_else(|| t_image::get_cached_preview_bytes(&file_path, options));
                    let mut response = match data {
                        Some(data) => image_response(data),
                        None => text_response(http::StatusCode::NOT_FOUND, "cached preview not found"),
                    };
                    response.headers_mut().insert(http::header::CACHE_CONTROL, http::HeaderValue::from_static("no-store"));
                    responder.respond(response);
                    return;
                }
                if for_editing {
                    // Use exactly the same pixels (including decoder fallbacks)
                    // that get_edited_image applies the crop and adjustments to.
                    let result = tauri::async_runtime::spawn_blocking(move || {
                        tauri::async_runtime::block_on(t_image::get_generated_preview_bytes(&file_path))
                    }).await;
                    responder.respond(match result {
                        Ok(Ok(Some(data))) => image_response(data),
                        _ => text_response(http::StatusCode::NOT_FOUND, "editable preview not found"),
                    });
                    return;
                }
                let pair_label = companion.as_ref().and_then(|file| file.file_path.as_ref()).map(|path| {
                    if t_image::is_heic_path(path) { "HEIC" } else { "JPEG" }
                }).unwrap_or("");
                if options.prefer_pair {
                    if let Some(path) = companion.and_then(|file| file.file_path) {
                        if let Ok(data) = t_image::get_file_image_bytes_cached(&path, options).await {
                            if image::load_from_memory(&data).is_ok() {
                                responder.respond(raw_preview_response(data, "pair", false, pair_label));
                                return;
                            }
                        }
                    }
                }
                if is_raw {
                    let full_size = request.uri().query().unwrap_or_default().split('&').any(|param| param == "rawFullSize=true");
                    let response = match t_image::get_raw_preview_cached(&file_path, options, full_size, raw_request).await {
                        Ok((data, source, unavailable)) => raw_preview_response(data, source, unavailable, pair_label),
                        Err(_) => text_response(http::StatusCode::NOT_FOUND, "RAW preview not found"),
                    };
                    responder.respond(response);
                    return;
                }
                let response = match t_image::get_file_image_bytes_cached(&file_path, options).await {
                    Ok(data) => image_response(data),
                    Err(_) => text_response(http::StatusCode::NOT_FOUND, "preview not found"),
                };
                responder.respond(response);
            });
        })
}

#[cfg(test)]
mod thumbnail_and_sdk_host_tests {
    use super::*;

    #[test]
    fn thumbnail_errors_distinguish_missing_files_decode_failures_and_stale_libraries() {
        let missing = thumbnail_error_response(http::StatusCode::NOT_FOUND, 4527, "source missing");
        assert_eq!(missing.status(), http::StatusCode::NOT_FOUND);
        assert_eq!(missing.headers()[http::header::CACHE_CONTROL], "no-store");
        assert!(String::from_utf8_lossy(missing.body()).contains("4527"));
        let decode = thumbnail_error_response(
            http::StatusCode::UNPROCESSABLE_ENTITY,
            7637,
            "decoder failed",
        );
        assert_eq!(decode.status(), http::StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            thumbnail_request_failure(7, "Library changed").status(),
            http::StatusCode::CONFLICT
        );
        assert_eq!(
            thumbnail_request_failure(7, "database busy").status(),
            http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }

}
