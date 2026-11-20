-- M1.1: one activation decision, explicit challenge lifecycle, and provenance tagging.

-- A challenge is a state machine, not a boolean.
alter table challenge add column if not exists state text not null default 'ISSUED';
do $$ begin
  alter table challenge add constraint challenge_state_check
    check (state in ('ISSUED', 'RESERVED', 'CONSUMED'));
exception when duplicate_object then null; end $$;

-- Legacy self-declarations are labelled, never silently promoted.
alter table usage_event add column if not exists source_class text not null default 'UNSIGNED_CLAIM';

create index if not exists usage_event_source_idx on usage_event (identity_id, source_class);
create index if not exists challenge_open_idx on challenge (id) where consumed_at_ms is null;
