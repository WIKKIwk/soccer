use axum::http::HeaderMap;
use football_contracts::{
    DEVELOPMENT_ADMIN_ID_HEADER, DEVELOPMENT_AUTHORIZATION_HEADER, DEVELOPMENT_AUTHORIZATION_VALUE,
};
use football_domain::{ActorIdentity, AdminUserId};
use football_platform::{ApplicationError, ErrorCode, RequestId};
use uuid::{Uuid, Version};

use crate::error::ApiCommandError;

/// Temporary development identity injection for the Week 8 vertical slice.
/// It fails closed and deliberately does not claim to be production login or
/// session authentication, which remains owned by IDN-003 through IDN-009.
pub(crate) fn development_actor(
    headers: &HeaderMap,
    request_id: RequestId,
) -> Result<ActorIdentity, ApiCommandError> {
    let authorization = headers
        .get(DEVELOPMENT_AUTHORIZATION_HEADER)
        .and_then(|value| value.to_str().ok());
    let actor_id = headers
        .get(DEVELOPMENT_ADMIN_ID_HEADER)
        .and_then(|value| value.to_str().ok());
    if authorization.is_none() || actor_id.is_none() {
        return Err(ApiCommandError::new(
            ApplicationError::new(
                ErrorCode::AuthenticationRequired,
                "development administrator headers are required",
            ),
            request_id,
            None,
        ));
    }
    if authorization != Some(DEVELOPMENT_AUTHORIZATION_VALUE) {
        return Err(ApiCommandError::new(
            ApplicationError::new(
                ErrorCode::AuthenticationInvalidSession,
                "development administrator authorization is invalid",
            ),
            request_id,
            None,
        ));
    }

    let actor_id = actor_id.expect("presence was checked");
    let uuid = Uuid::parse_str(actor_id).ok().filter(|uuid| {
        uuid.get_version() == Some(Version::SortRand) && uuid.to_string() == actor_id
    });
    let Some(uuid) = uuid else {
        return Err(ApiCommandError::new(
            ApplicationError::new(
                ErrorCode::AuthenticationInvalidSession,
                "development administrator identity is invalid",
            ),
            request_id,
            None,
        ));
    };
    Ok(ActorIdentity::administrator(AdminUserId::from_uuid(uuid)))
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};
    use football_contracts::{
        DEVELOPMENT_ADMIN_ID_HEADER, DEVELOPMENT_AUTHORIZATION_HEADER,
        DEVELOPMENT_AUTHORIZATION_VALUE,
    };
    use football_platform::RequestId;

    use super::development_actor;

    #[test]
    fn development_actor_requires_both_exact_headers() {
        let mut headers = HeaderMap::new();
        assert!(development_actor(&headers, RequestId::new()).is_err());

        headers.insert(
            DEVELOPMENT_AUTHORIZATION_HEADER,
            HeaderValue::from_static(DEVELOPMENT_AUTHORIZATION_VALUE),
        );
        headers.insert(
            DEVELOPMENT_ADMIN_ID_HEADER,
            HeaderValue::from_static("019ff700-3bc5-7cf0-b3d2-0728c077a930"),
        );
        assert!(development_actor(&headers, RequestId::new()).is_ok());

        headers.insert(
            DEVELOPMENT_AUTHORIZATION_HEADER,
            HeaderValue::from_static("true"),
        );
        assert!(development_actor(&headers, RequestId::new()).is_err());
    }
}
