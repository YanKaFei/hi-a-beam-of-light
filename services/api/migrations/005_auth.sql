-- M3-B: business sessions for the mini program. No WeChat secret is stored here.

create table if not exists wechat_subject (
    id           text primary key,
    app_id       text not null,
    open_id      text not null,
    identity_id  text not null references identity(id) on delete cascade,
    created_at_ms bigint not null,
    unique (app_id, open_id)
);

create table if not exists auth_session (
    id            text primary key,
    identity_id   text not null references identity(id) on delete cascade,
    token_hash    text not null unique,
    auth_method   text not null check (auth_method in ('WECHAT','DEV_ADAPTER')),
    created_at_ms bigint not null,
    expires_at_ms bigint not null,
    revoked_at_ms bigint
);

create index if not exists auth_session_identity_idx on auth_session (identity_id);

-- wx.login codes are single-use. Kept in their own table so a constraint violation
-- means exactly one thing: this code was already exchanged.
create table if not exists wechat_code (
    code_hash     text primary key,
    consumed_at_ms bigint not null
);
