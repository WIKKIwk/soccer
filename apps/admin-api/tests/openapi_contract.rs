const ADMIN_OPENAPI: &str = include_str!("../../../openapi/admin-v1.yaml");

#[test]
fn admin_openapi_exposes_the_complete_regular_time_command_surface() {
    for required in [
        "/admin/v1/matches/{match_id}/schedule:",
        "/admin/v1/matches/{match_id}/lineups/submit:",
        "/admin/v1/matches/{match_id}/lineups/confirm:",
        "/admin/v1/matches/{match_id}/periods/first-half/start:",
        "/admin/v1/matches/{match_id}/periods/first-half/end:",
        "/admin/v1/matches/{match_id}/periods/second-half/start:",
        "/admin/v1/matches/{match_id}/finish:",
        "/admin/v1/matches/{match_id}/goals:",
        "/admin/v1/matches/{match_id}/cards:",
        "/admin/v1/matches/{match_id}/substitutions:",
        "CommandMetadata:",
        "AdminCommandResponse:",
        "x-development-authorization",
        "operator_lease_id",
        "expected_version",
    ] {
        assert!(
            ADMIN_OPENAPI.contains(required),
            "admin OpenAPI is missing {required:?}"
        );
    }
}
