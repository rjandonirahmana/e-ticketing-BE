use crate::web::models::*;
use leptos::prelude::*;
#[cfg_attr(not(feature = "ssr"), allow(unused_imports))]
use super::helpers::*;

#[server(GetProducts, "/api-fn")]
pub async fn get_products(
    page: Option<i64>,
    city: Option<String>,
    category: Option<String>,
    search: Option<String>,
    per_page: Option<i64>,
) -> Result<PaginatedProducts, ServerFnError> {
    use crate::models::products::ProductListQuery;
    let state = app_state().await?;

    let cache_key = format!(
        "{}|{}|{}|{}|{}",
        page.unwrap_or(1),
        city.as_deref().unwrap_or(""),
        category.as_deref().unwrap_or(""),
        search.as_deref().unwrap_or(""),
        per_page.unwrap_or(20),
    );
    if let Some(cached) = state.pub_cache.products.get(&cache_key).await {
        return Ok(cached);
    }

    let q = ProductListQuery {
        sort: None,
        page,
        per_page,
        city,
        category,
        search,
        status: Some("active".into()),
    };
    let result = state.product_svc.list(q, None).await.map_err(map_app_error)?;
    let web = srv_paginated_products_to_web(result);
    state.pub_cache.products.insert(cache_key, web.clone()).await;
    Ok(web)
}

#[server(GetProductDetail, "/api-fn")]
pub async fn get_product_detail(slug: String) -> Result<ProductWithVariants, ServerFnError> {
    let state = app_state().await?;
    if let Some(cached) = state.pub_cache.product_detail.get(&slug).await {
        return Ok(cached);
    }
    let result = state.product_svc.get(&slug).await.map_err(map_app_error)?;
    let web = srv_product_with_variants_to_web(result);
    state.pub_cache.product_detail.insert(slug, web.clone()).await;
    Ok(web)
}

#[server(GetCategories, "/api-fn")]
pub async fn get_categories() -> Result<Vec<String>, ServerFnError> {
    let state = app_state().await?;
    if let Some(cached) = state.pub_cache.categories.get(&()).await {
        return Ok(cached);
    }
    let cats = state
        .product_svc
        .list_categories()
        .await
        .map_err(map_app_error)?;
    state.pub_cache.categories.insert((), cats.clone()).await;
    Ok(cats)
}

#[server(GetBanners, "/api-fn")]
pub async fn get_banners() -> Result<Vec<Banner>, ServerFnError> {
    let state = app_state().await?;
    if let Some(cached) = state.pub_cache.banners.get(&()).await {
        return Ok(cached.into_iter().map(srv_banner_to_web).collect());
    }
    let banners = state
        .banner_svc
        .list_active(None)
        .await
        .map_err(map_app_error)?;
    state.pub_cache.banners.insert((), banners.clone()).await;
    Ok(banners.into_iter().map(srv_banner_to_web).collect())
}

#[server(GetProductLocation, "/api-fn")]
pub async fn get_product_location(slug: String) -> Result<serde_json::Value, ServerFnError> {
    let state = app_state().await?;
    let product = state
        .product_svc
        .get(&slug)
        .await
        .map_err(map_app_error)?;
    return Ok(serde_json::json!({
        "venue": product.venue,
        "city": product.city,
        "slug": product.slug,
        "latitude": product.latitude,
        "longitude": product.longitude,
    }));
}
