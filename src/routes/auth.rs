// standard
use std::borrow::Borrow;
use std::sync::Mutex;

// thirdparty
use actix_web::{
    cookie::Cookie, delete, http::StatusCode, post, put, web::Data, HttpResponse, Responder,
};
use actix_web_validator::{Json, Query, Validate};
use diesel::dsl::{exists, select};
use diesel::*;
use serde::{Deserialize, Serialize};

// in crate
use crate::lib;
use crate::lib::jwt;
use crate::models::{InsertRefreshToken, InsertUser, SelectUser};
use crate::response::ServerErrorResponse;
use crate::schema::{tb_refresh_token, tb_user};

#[derive(Deserialize, Validate, Debug)]
pub struct SignupParam {
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 8))]
    pub password: String,
    pub nickname: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct SignupResponse {
    pub success: bool,
    pub email_duplicated: bool,
    pub message: String,
}

// 회원가입
#[post("/auth/signup")]
pub async fn signup(
    Json(body): Json<SignupParam>,
    connection: Data<Mutex<PgConnection>>,
) -> impl Responder {
    let connection = match connection.lock() {
        Err(error) => {
            log::error!("database connection lock error");
            sentry::capture_error(&error);
            let response = ServerErrorResponse::new();
            return HttpResponse::build(StatusCode::INTERNAL_SERVER_ERROR).json(response);
        }
        Ok(connection) => connection,
    };
    let connection: &PgConnection = Borrow::borrow(&connection);

    // 이메일 중복 체크
    let already_exists = select(exists(
        tb_user::dsl::tb_user.filter(tb_user::dsl::email.eq(body.email.clone())),
    ))
    .get_result(connection);

    match already_exists {
        Ok(already_exists) => {
            if already_exists {
                let response = SignupResponse {
                    success: false,
                    email_duplicated: true,
                    message: "email already exists".to_owned(),
                };
                return HttpResponse::build(StatusCode::OK).json(response);
            }
        }
        Err(error) => {
            log::error!("error: {}", error);
            sentry::capture_error(&error);
            let response = ServerErrorResponse::new();
            return HttpResponse::build(StatusCode::INTERNAL_SERVER_ERROR).json(response);
        }
    }

    // 회원가입 데이터 삽입
    let insert_value = InsertUser::new(body.email, body.password, body.nickname);

    let execute_result = diesel::insert_into(tb_user::table)
        .values(insert_value)
        .execute(connection);

    match execute_result {
        Ok(_) => {
            let response = SignupResponse {
                success: true,
                email_duplicated: false,
                message: "".to_owned(),
            };
            HttpResponse::build(StatusCode::OK).json(response)
        }
        Err(error) => {
            log::error!("error: {}", error);
            sentry::capture_error(&error);
            let response = ServerErrorResponse::new();
            return HttpResponse::build(StatusCode::INTERNAL_SERVER_ERROR).json(response);
        }
    }
}

#[derive(Deserialize, Validate, Debug)]
pub struct LoginParam {
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 8))]
    pub password: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct LoginResponse {
    pub success: bool,
    pub login_failed: bool,
    pub access_token: String,
    pub refresh_token: String,
    pub message: String,
}

// 로그인
#[post("/auth/login")]
pub async fn login(
    Json(body): Json<LoginParam>,
    connection: Data<Mutex<PgConnection>>,
) -> impl Responder {
    let connection = match connection.lock() {
        Err(error) => {
            log::error!("database connection lock error");
            sentry::capture_error(&error);
            let response = ServerErrorResponse::new();
            return HttpResponse::build(StatusCode::INTERNAL_SERVER_ERROR).json(response);
        }
        Ok(connection) => connection,
    };
    let connection: &PgConnection = Borrow::borrow(&connection);

    let LoginParam { email, password } = body;

    let query = tb_user::dsl::tb_user
        .filter(tb_user::dsl::email.eq(&email))
        .filter(tb_user::dsl::use_yn.eq(true));

    let user_result = query.load::<SelectUser>(connection);

    match user_result {
        Ok(users) => {
            if users.is_empty() {
                let response = LoginResponse {
                    success: false,
                    login_failed: true,
                    access_token: "".to_owned(),
                    refresh_token: "".to_owned(),
                    message: "login failed".to_owned(),
                };

                HttpResponse::build(StatusCode::OK).json(response)
            } else {
                let user = &users[0];
                let salt = &user.salt;

                let password = lib::hash(password + salt);

                if password == user.password {
                    // 리프레시 토큰 생성 및 DB에 삽입
                    let refresh_token =
                        lib::jwt::create_refresh_token(user.id, user.user_type.clone());

                    let insert_value = InsertRefreshToken {
                        token_value: refresh_token.clone(),
                        user_id: user.id,
                    };
                    let execute_result = diesel::insert_into(tb_refresh_token::table)
                        .values(insert_value)
                        .execute(connection);

                    if execute_result.is_err() {
                        log::error!("refresh token insert query error");
                        let response = ServerErrorResponse::new();
                        return HttpResponse::build(StatusCode::INTERNAL_SERVER_ERROR)
                            .json(response);
                    }

                    use actix_web::cookie::Cookie;

                    // 액세스 토큰 생성
                    let access_token =
                        lib::jwt::create_access_token(user.id, user.user_type.clone());

                    let access_token_cookie = Cookie::build("access_token", access_token.clone())
                        .http_only(true)
                        .max_age(time::Duration::hours(1))
                        .path("/")
                        .finish();

                    // let refresh_token_cookie =
                    //     Cookie::build("refresh_token", refresh_token.clone())
                    //         .http_only(true)
                    //         .max_age(time::Duration::days(365))
                    //         .path("/")
                    //         .finish();

                    let response = LoginResponse {
                        success: true,
                        login_failed: false,
                        access_token: access_token,
                        refresh_token: refresh_token,
                        message: "success".to_owned(),
                    };

                    let mut http_response = HttpResponse::build(StatusCode::OK).json(response);
                    http_response.add_cookie(&access_token_cookie).unwrap_or(());
                    http_response
                } else {
                    log::info!("로그인 실패: 패스워드 불일치");
                    let response = LoginResponse {
                        success: false,
                        login_failed: true,
                        access_token: "".to_owned(),
                        refresh_token: "".to_owned(),
                        message: "login failed".to_owned(),
                    };

                    HttpResponse::build(StatusCode::OK).json(response)
                }
            }
        }
        Err(error) => {
            log::error!("login select query error: {}", error);
            sentry::capture_error(&error);
            let response = LoginResponse {
                success: false,
                login_failed: false,
                access_token: "".to_owned(),
                refresh_token: "".to_owned(),
                message: error.to_string(),
            };

            HttpResponse::build(StatusCode::INTERNAL_SERVER_ERROR).json(response)
        }
    }
}

#[derive(Deserialize, Validate, Debug)]
pub struct LogoutParam {
    pub refresh_token: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct LogoutResponse {
    pub success: bool,
    pub message: String,
}

// 로그아웃
#[delete("/auth/logout")]
pub async fn logout(
    Query(query): Query<LogoutParam>,
    connection: Data<Mutex<PgConnection>>,
) -> impl Responder {
    let connection = match connection.lock() {
        Err(_) => {
            log::error!("database connection lock error");
            let response = ServerErrorResponse::new();
            return HttpResponse::build(StatusCode::INTERNAL_SERVER_ERROR).json(response);
        }
        Ok(connection) => connection,
    };
    let connection: &PgConnection = Borrow::borrow(&connection);

    let token = tb_refresh_token::dsl::tb_refresh_token
        .filter(tb_refresh_token::dsl::token_value.eq(&query.refresh_token))
        .filter(tb_refresh_token::dsl::dead_yn.eq(false));

    // 리프레시 토큰 삭제처리
    let result = connection.transaction(|| {
        diesel::update(token)
            .set(tb_refresh_token::dsl::dead_yn.eq_all(true))
            .execute(connection)?;
        diesel::update(token)
            .set(tb_refresh_token::dsl::dead_utc.eq_all(epoch_timestamp::Epoch::now() as i64))
            .execute(connection)
    });

    match result {
        Ok(_) => {
            let response = LogoutResponse {
                success: true,
                message: "logout success".to_owned(),
            };

            let mut http_response = HttpResponse::build(StatusCode::OK).json(response);

            let access_token_cookie = Cookie::build("access_token", "")
                .http_only(true)
                .max_age(time::Duration::seconds(0))
                .path("/")
                .finish();

            http_response.add_cookie(&access_token_cookie).unwrap_or(());

            http_response
        }
        Err(error) => {
            log::error!("logout error: {}", error);
            sentry::capture_error(&error);
            let response = LogoutResponse {
                success: false,
                message: error.to_string(),
            };

            HttpResponse::build(StatusCode::INTERNAL_SERVER_ERROR).json(response)
        }
    }
}

#[derive(Deserialize, Validate, Debug)]
pub struct RefreshParam {
    pub refresh_token: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct RefreshResponse {
    pub success: bool,
    pub expired: bool,
    pub access_token: String,
    pub message: String,
}

// 액세스 토큰 갱신
#[put("/auth/refresh")]
pub async fn refresh(
    Json(body): Json<RefreshParam>,
    connection: Data<Mutex<PgConnection>>,
) -> impl Responder {
    let connection = match connection.lock() {
        Err(error) => {
            log::error!("database connection lock error");
            sentry::capture_error(&error);
            let response = ServerErrorResponse::new();
            return HttpResponse::build(StatusCode::INTERNAL_SERVER_ERROR).json(response);
        }
        Ok(connection) => connection,
    };
    let connection: &PgConnection = Borrow::borrow(&connection);

    use diesel::dsl::{exists, select};

    let query = select(exists(
        tb_refresh_token::dsl::tb_refresh_token
            .filter(tb_refresh_token::dsl::token_value.eq(&body.refresh_token))
            .filter(tb_refresh_token::dsl::dead_yn.eq(false)),
    ));

    let result = query.get_result(connection);

    match result {
        Ok(exists) => {
            if exists {
                let decoded_result = jwt::verify(body.refresh_token);

                let response = match decoded_result {
                    Some(user_id) => {
                        let query = tb_user::dsl::tb_user.filter(tb_user::dsl::id.eq(&user_id));

                        let result = query.load::<SelectUser>(connection);

                        match result {
                            Ok(select_user) => {
                                if select_user.is_empty() {
                                    let response = RefreshResponse {
                                        success: false,
                                        expired: false,
                                        access_token: "".into(),
                                        message: "user not exists".to_owned(),
                                    };

                                    HttpResponse::build(StatusCode::OK).json(response)
                                } else {
                                    let user_type = select_user[0].user_type.clone();

                                    // 액세스 토큰 생성
                                    let access_token =
                                        lib::jwt::create_access_token(user_id, user_type);

                                    let access_token_cookie =
                                        Cookie::build("access_token", access_token.clone())
                                            .http_only(true)
                                            .max_age(time::Duration::hours(1))
                                            .path("/")
                                            .finish();

                                    let response = RefreshResponse {
                                        success: true,
                                        expired: false,
                                        access_token: access_token,
                                        message: "refresh success".to_owned(),
                                    };

                                    let mut http_response =
                                        HttpResponse::build(StatusCode::OK).json(response);

                                    http_response.add_cookie(&access_token_cookie).unwrap_or(());

                                    http_response
                                }
                            }
                            Err(error) => {
                                log::error!("database error: {:?}", error);
                                sentry::capture_error(&error);
                                let response = ServerErrorResponse::new();
                                HttpResponse::build(StatusCode::OK).json(response)
                            }
                        }
                    }
                    None => {
                        let response = RefreshResponse {
                            success: false,
                            expired: true,
                            access_token: "".into(),
                            message: "logout failed".to_owned(),
                        };
                        HttpResponse::build(StatusCode::OK).json(response)
                    }
                };

                response
            } else {
                let response = RefreshResponse {
                    success: false,
                    expired: true,
                    access_token: "".into(),
                    message: "logout failed".to_owned(),
                };
                HttpResponse::build(StatusCode::OK).json(response)
            }
        }
        Err(error) => {
            log::error!("database error: {:?}", error);
            sentry::capture_error(&error);
            let response = ServerErrorResponse::new();
            return HttpResponse::build(StatusCode::INTERNAL_SERVER_ERROR).json(response);
        }
    }
}
