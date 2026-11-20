-- M2.1: device identity bound to a subject, plus a two-sided pairing protocol.

-- A device belongs to nobody until pairing succeeds, so identity_id must be optional.
alter table device_key alter column identity_id drop not null;

alter table device_key add column if not exists subject_identity_id text
  references identity(id) on delete cascade;
alter table device_key add column if not exists status_v2 text not null default 'PENDING';
do $$ begin
  alter table device_key add constraint device_key_status_v2_check
    check (status_v2 in ('PENDING', 'ACTIVE', 'REVOKED'));
exception when duplicate_object then null; end $$;

create table if not exists pairing_session (
    id              text primary key,
    device_id       text   not null,                     -- device_key.id
    device_pubkey   text   not null,
    code_hash       text   not null,                     -- short code is never stored raw
    state           text   not null default 'OFFERED'
                            check (state in ('OFFERED','APPROVED','CONSUMED','CANCELLED','EXPIRED')),
    subject_identity_id text references identity(id) on delete set null,
    created_at_ms   bigint not null,
    expires_at_ms   bigint not null,
    approved_at_ms  bigint,
    consumed_at_ms  bigint
);

create index if not exists pairing_state_idx on pairing_session (state, expires_at_ms);
