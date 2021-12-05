// standard
use std::borrow::Borrow;
use std::sync::Mutex;

// thirdparty
use actix_web::{get, http::StatusCode, post, web::Data, HttpRequest, HttpResponse, Responder};
use actix_web_validator::{Json, Query, Validate};
use diesel::*;
use serde::{Deserialize, Serialize};

// in crate
use crate::lib::{init_pagination, to_page_token, AuthValue};
use crate::models::{InsertDebate, InsertDebateComment};
use crate::response::{ServerErrorResponse, UnauthorizedResponse};
use crate::schema::{tb_debate, tb_debate_comment, tb_document, tb_user};
use crate::value::{Debate, DebateComment};

#[derive(Deserialize, Validate, Debug)]
pub struct RenderDocumentParam {
    pub content: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct RenderDocumentResponse {
    pub success: bool,
    pub html: String,
}

#[post("/grammar/render")]
pub async fn render_document(
    Json(body): Json<RenderDocumentParam>,
    _request: HttpRequest,
    _connection: Data<Mutex<PgConnection>>,
) -> impl Responder {
    let _raw_content = body.content;

    // TODO: 문법 분석 후 html 생성

    let html = "";

    let response = RenderDocumentResponse {
        success: true,
        html: html.to_string(),
    };
    HttpResponse::build(StatusCode::OK).json(response)
}
