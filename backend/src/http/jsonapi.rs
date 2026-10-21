//! The only structured success-response boundary. Wire models belong to Cairnkit.
use crate::business::{Error, Page, Result};
use axum::{
    Json,
    http::header,
    response::{IntoResponse, Response},
};
use cairnkit_jsonapi::api::{DocumentData, JSON_API_MEDIA_TYPE, PrimaryData, Resource};
use serde::Serialize;
use serde_json::{Value, json};

pub struct ApiResponse(Result<DocumentData>);

fn model(kind: &str, value: Value) -> Result<Resource> {
    let Value::Object(mut attributes) = value else {
        return Err(Error::Database("resource must be an object".into()));
    };
    let id = attributes
        .remove("id")
        .and_then(|id| id.as_str().map(str::to_owned))
        .filter(|id| !id.is_empty())
        .ok_or_else(|| Error::Database(format!("missing resource id: {kind}")))?;
    Ok(Resource {
        _type: kind.into(),
        id: Some(id),
        attributes: attributes.into_iter().collect(),
        ..Default::default()
    })
}

pub fn resource(kind: &str, value: impl Serialize) -> ApiResponse {
    ApiResponse((|| {
        let value = serde_json::to_value(value).map_err(|e| Error::Database(e.to_string()))?;
        let data = match value {
            Value::Array(items) => PrimaryData::Multiple(
                items
                    .into_iter()
                    .map(|v| model(kind, v))
                    .collect::<Result<_>>()?,
            ),
            item => PrimaryData::Single(Box::new(model(kind, item)?)),
        };
        Ok(DocumentData {
            data: Some(data),
            ..Default::default()
        })
    })())
}

/// Stable singleton IDs are explicit, never generated per response.
pub fn singleton(kind: &str, id: impl ToString, value: impl Serialize) -> ApiResponse {
    match serde_json::to_value(value) {
        Ok(Value::Object(mut attributes)) => {
            attributes.insert("id".into(), Value::String(id.to_string()));
            resource(kind, attributes)
        }
        _ => ApiResponse(Err(Error::Database("invalid singleton attributes".into()))),
    }
}

pub fn completed() -> ApiResponse {
    ApiResponse(Ok(DocumentData {
        data: Some(PrimaryData::None),
        ..Default::default()
    }))
}

pub fn collection(kind: &str, mut items: Vec<Value>, page: &Page) -> ApiResponse {
    let size = page.page_size.unwrap_or(30) as usize;
    let has_more = items.len() > size;
    items.truncate(size);
    resource(kind, items).meta(
        json!({"page": {"number": page.page.unwrap_or(1), "size": size, "has_more": has_more}}),
    )
}

impl ApiResponse {
    pub fn meta(mut self, meta: Value) -> Self {
        if let Ok(document) = &mut self.0 {
            document.meta.get_or_insert_with(Default::default).extend(
                meta.as_object()
                    .into_iter()
                    .flatten()
                    .map(|(k, v)| (k.clone(), v.clone())),
            );
        }
        self
    }
}
impl IntoResponse for ApiResponse {
    fn into_response(self) -> Response {
        match self.0 {
            Ok(document) => (
                [(header::CONTENT_TYPE, JSON_API_MEDIA_TYPE)],
                Json(document),
            )
                .into_response(),
            Err(error) => error.into_response(),
        }
    }
}

/// JSON:API write document. Resource models are reused from Cairnkit, not redefined.
pub struct Input(Resource);
impl<S: Send + Sync> axum::extract::FromRequest<S> for Input {
    type Rejection = Response;
    async fn from_request(
        request: axum::extract::Request,
        state: &S,
    ) -> std::result::Result<Self, Response> {
        if request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            != Some(JSON_API_MEDIA_TYPE)
        {
            return Err(super::error::response(
                axum::http::StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported_media_type",
                "请使用 application/vnd.api+json",
            ));
        }
        let Json(document) = Json::<DocumentData>::from_request(request, state)
            .await
            .map_err(IntoResponse::into_response)?;
        match document.data {
            Some(PrimaryData::Single(resource))
                if document.included.is_none() && resource.relationships.is_none() =>
            {
                Ok(Self(*resource))
            }
            _ => Err(Error::Invalid("写入需要单个 JSON:API 资源".into()).into_response()),
        }
    }
}
impl Input {
    pub fn decode<T: serde::de::DeserializeOwned>(self, kind: &str) -> Result<T> {
        let mut resource = self.0;
        crate::business::require(resource._type == kind, "资源类型与接口不匹配")?;
        crate::business::require(
            !resource.attributes.contains_key("id") && !resource.attributes.contains_key("type"),
            "id 和 type 必须放在资源顶层",
        )?;
        if let Some(id) = resource.id {
            crate::business::require(!id.is_empty(), "资源 ID 不能为空")?;
            resource.attributes.insert("id".into(), Value::String(id));
        }
        serde_json::from_value(json!(resource.attributes))
            .map_err(|_| Error::Invalid("请求字段类型错误或缺少必填字段".into()))
    }
}
