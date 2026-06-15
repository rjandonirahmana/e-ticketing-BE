use crate::web::models::*;
use leptos::prelude::*;
use super::helpers::*;

#[server(GetMe, "/api-fn")]
pub async fn get_me() -> Result<UserResponse, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let u = state
        .auth_svc
        .me(&claims.user_id)
        .await
        .map_err(map_app_error)?;
    return Ok(srv_user_to_web(u));
}
