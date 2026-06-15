// ═══════════════════════════════════════════════════════════════════════════════
//  STORY — Upload helper
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(target_arch = "wasm32")]
pub(super) async fn upload_story_file(
    file: &web_sys::File,
    slug: Option<String>,
) -> Result<(), String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let form = web_sys::FormData::new().map_err(|e| format!("{:?}", e))?;
    form.append_with_blob("media", file).map_err(|e| format!("{:?}", e))?;
    if let Some(s) = slug {
        form.append_with_str("slug", &s).map_err(|e| format!("{:?}", e))?;
    }

    let opts = web_sys::RequestInit::new();
    opts.set_method("POST");
    opts.set_body(&form);

    let req = web_sys::Request::new_with_str_and_init("/upload/story", &opts)
        .map_err(|e| format!("{:?}", e))?;

    let win = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_val = JsFuture::from(win.fetch_with_request(&req))
        .await
        .map_err(|e| format!("{:?}", e))?;
    let resp: web_sys::Response = resp_val.unchecked_into();

    if resp.ok() {
        Ok(())
    } else {
        Err(format!("Upload gagal: HTTP {}", resp.status()))
    }
}
