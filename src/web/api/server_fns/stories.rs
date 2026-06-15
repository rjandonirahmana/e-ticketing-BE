use leptos::prelude::*;
#[cfg(feature = "ssr")]
use super::helpers::*;

#[server(GetStoryGroups, "/api-fn")]
pub async fn get_story_groups(
) -> Result<Vec<crate::web::state::stories::StoryGroup>, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let groups = state
        .story_svc
        .list_groups(&claims.user_id)
        .await
        .map_err(map_app_error)?;
    return Ok(srv_story_groups_to_web(groups));
}
