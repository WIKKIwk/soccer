CREATE TABLE admin_users (
    id UUID PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT admin_users_username_not_blank CHECK (char_length(btrim(username)) > 0),
    CONSTRAINT admin_users_display_name_not_blank CHECK (char_length(btrim(display_name)) > 0)
);
