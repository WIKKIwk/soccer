const PUBLIC_OPENAPI: &str = include_str!("../../../openapi/public-v1.yaml");

#[test]
fn public_openapi_keeps_both_canonical_match_routes_and_versioned_envelopes() {
    for required in [
        "/v1/matches/{match_id}:",
        "/v1/matches/{match_id}/timeline:",
        "operationId: getMatchDetail",
        "operationId: getMatchTimeline",
        "MatchSnapshotResponse:",
        "MatchTimelineResponse:",
        "verification_status:",
        "quality_flags:",
        "ApiErrorResponse:",
    ] {
        assert!(
            PUBLIC_OPENAPI.contains(required),
            "public OpenAPI is missing {required:?}"
        );
    }
}
