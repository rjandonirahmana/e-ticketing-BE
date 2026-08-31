use crate::web::models::*;
use leptos::prelude::*;
#[cfg_attr(not(feature = "ssr"), allow(unused_imports))]
use super::helpers::*;
#[cfg(feature = "ssr")]
use super::session::{clear_auth_cookie, set_auth_cookie, set_refresh_cookie};

#[server(LoginAction, "/api-fn")]
pub async fn login_action(phone: String, password: String) -> Result<UserResponse, ServerFnError> {
    use crate::models::users::LoginRequest;
    let state = app_state().await?;
    let req = LoginRequest { phone, password };
    let auth = state.auth_svc.login(req).await.map_err(map_app_error)?;

    // Cookie refresh menemani cookie access. Tanpanya, sesi web berakhir
    // begitu access token mati — dan menaikkan umur access token demi
    // kenyamanan justru memperpanjang umur peran yang sudah dicabut.
    if let Ok(rt) = state
        .refresh_svc
        .issue(&auth.user.id, None, "web")
        .await
    {
        set_refresh_cookie(&rt);
    }

    set_auth_cookie(&auth.access_token);
    return Ok(srv_user_to_web(auth.user));
}

#[server(RegisterAction, "/api-fn")]
pub async fn register_action(
    name: String,
    phone: String,
    role: String,
) -> Result<(), ServerFnError> {
    use crate::models::users::RegisterRequest;
    let state = app_state().await?;
    let req = RegisterRequest {
        name,
        phone,
        email: None,
        role: Some(role),
    };
    return state
        .auth_svc
        .initiate_register(req)
        .await
        .map_err(map_app_error);
}

#[server(VerifyOtpAction, "/api-fn")]
pub async fn verify_otp_action(phone: String, otp: String) -> Result<UserResponse, ServerFnError> {
    let state = app_state().await?;
    let auth = state
        .auth_svc
        .verify_register(&phone, &otp)
        .await
        .map_err(map_app_error)?;

    if let Ok(rt) = state.refresh_svc.issue(&auth.user.id, None, "web").await {
        set_refresh_cookie(&rt);
    }

    set_auth_cookie(&auth.access_token);
    return Ok(srv_user_to_web(auth.user));
}

/// Perbarui profil (nama). Nomor HP TIDAK lewat sini — lihat
/// `mulai_ganti_nomor_action`.
#[server(UpdateMyProfile, "/api-fn")]
pub async fn update_my_profile(
    name: String,
    email: Option<String>,
) -> Result<UserResponse, ServerFnError> {
    use crate::models::users::UpdateProfileRequest;
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let req = UpdateProfileRequest {
        name: Some(name),
        phone: None,
        email,
    };
    state
        .auth_svc
        .update_profile(&claims.user_id, req)
        .await
        .map(srv_user_to_web)
        .map_err(map_app_error)
}

/// Ajukan ganti nomor: kode dikirim ke NOMOR BARU.
#[server(MulaiGantiNomor, "/api-fn")]
pub async fn mulai_ganti_nomor_action(phone: String) -> Result<String, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    state
        .auth_svc
        .mulai_ganti_nomor(&claims.user_id, &phone)
        .await
        .map_err(map_app_error)
}

/// Verifikasi kode; bila cocok, nomor akun benar-benar berpindah.
#[server(VerifikasiGantiNomor, "/api-fn")]
pub async fn verifikasi_ganti_nomor_action(otp: String) -> Result<String, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    state
        .auth_svc
        .verifikasi_ganti_nomor(&claims.user_id, &otp)
        .await
        .map_err(map_app_error)
}

/// Lupa password: kirim password baru ke WhatsApp nomor tersebut.
///
/// Sandi LAMA tidak disentuh. Ia baru berganti ketika seseorang benar-benar
/// masuk memakai sandi barunya — menekan tombol ini hanya butuh mengetik nomor
/// HP orang lain, jadi permintaannya sendiri tak boleh mengubah apa pun.
#[server(ForgotPasswordAction, "/api-fn")]
pub async fn forgot_password_action(phone: String) -> Result<String, ServerFnError> {
    let state = app_state().await?;
    state
        .auth_svc
        .forgot_password(&phone)
        .await
        .map_err(map_app_error)
}

#[server(ResendOtpAction, "/api-fn")]
pub async fn resend_otp_action(name: String, phone: String) -> Result<(), ServerFnError> {
    use crate::models::users::RegisterRequest;
    let state = app_state().await?;
    let req = RegisterRequest {
        name,
        phone,
        email: None,
        role: Some("customer".into()),
    };
    // Ignore errors (rate limit etc.) — just fire off another OTP
    let _ = state.auth_svc.initiate_register(req).await;
    return Ok(());
}

#[server(LogoutAction, "/api-fn")]
pub async fn logout_action() -> Result<(), ServerFnError> {
    // Cabut di SERVER lebih dulu, baru hapus cookie. Menghapus cookie saja
    // hanya membuat peramban lupa — refresh tokennya tetap sah, dan siapa pun
    // yang sempat menyalinnya masih bisa menukarnya jadi sesi baru.
    if let Some(rt) = super::session::get_refresh_token().await {
        if let Ok(state) = app_state().await {
            if let Err(e) = state.refresh_svc.revoke(&rt).await {
                tracing::warn!(error = %e, "logout web: gagal mencabut refresh token");
            }
        }
    }
    clear_auth_cookie();
    return Ok(());
}
