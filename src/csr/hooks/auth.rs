use leptos::prelude::*;
use leptos::task::spawn_local;
use gloo_storage::{LocalStorage, Storage};

use crate::csr::models::{LoginRequest, LogoutRequest, UserProfile, VerifyOtpRequest};
use crate::csr::services::{auth as auth_svc, ApiError};
use crate::csr::services::client::{TOKEN_KEY, REFRESH_KEY, USER_KEY};

#[derive(Clone, Copy)]
pub struct AuthCtx {
    pub user: RwSignal<Option<UserProfile>>,
    pub access_token: RwSignal<Option<String>>,
    pub is_loading: RwSignal<bool>,
}

impl AuthCtx {
    pub fn is_authenticated(&self) -> bool {
        self.access_token.with(|t| t.is_some())
    }

    pub async fn login(&self, phone: String, password: String) -> Result<(), ApiError> {
        let res = auth_svc::login(&LoginRequest { phone, password }).await?;
        let _ = LocalStorage::set(TOKEN_KEY, &res.access_token);
        let _ = LocalStorage::set(REFRESH_KEY, &res.refresh_token);
        let _ = LocalStorage::set(USER_KEY, &res.user);
        self.access_token.set(Some(res.access_token));
        self.user.set(Some(res.user));
        self.is_loading.set(false);
        Ok(())
    }

    /// Confirm OTP issued by `/auth/register` and log the user in.
    pub async fn verify_otp(&self, phone: String, otp: String) -> Result<(), ApiError> {
        let res = auth_svc::verify_register(&VerifyOtpRequest { phone, otp }).await?;
        let _ = LocalStorage::set(TOKEN_KEY, &res.access_token);
        let _ = LocalStorage::set(REFRESH_KEY, &res.refresh_token);
        let _ = LocalStorage::set(USER_KEY, &res.user);
        self.access_token.set(Some(res.access_token));
        self.user.set(Some(res.user));
        self.is_loading.set(false);
        Ok(())
    }

    pub fn logout(&self) {
        let token = LocalStorage::get::<String>(TOKEN_KEY).ok();
        if let Some(tok) = token {
            spawn_local(async move {
                let _ = auth_svc::logout(&LogoutRequest { access_token: tok }).await;
            });
        }
        LocalStorage::delete(TOKEN_KEY);
        LocalStorage::delete(REFRESH_KEY);
        LocalStorage::delete(USER_KEY);
        self.access_token.set(None);
        self.user.set(None);
    }
}

pub fn provide_auth() {
    let user_signal: RwSignal<Option<UserProfile>> = RwSignal::new(None);
    let token_signal: RwSignal<Option<String>> = RwSignal::new(None);
    let loading_signal = RwSignal::new(true);

    // Rehydrate from localStorage
    if let (Ok(token), Ok(user)) = (
        LocalStorage::get::<String>(TOKEN_KEY),
        LocalStorage::get::<UserProfile>(USER_KEY),
    ) {
        token_signal.set(Some(token));
        user_signal.set(Some(user));
    }
    loading_signal.set(false);

    provide_context(AuthCtx {
        user: user_signal,
        access_token: token_signal,
        is_loading: loading_signal,
    });
}

pub fn use_auth() -> AuthCtx {
    use_context::<AuthCtx>().expect("AuthCtx not provided — call provide_auth()")
}
