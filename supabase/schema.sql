-- RustWeb certification registry for Supabase.
--
-- Run this whole file once in Supabase Dashboard -> SQL Editor -> Run.
-- The browser uses only the publishable key. The secret key is not needed
-- by the frontend and must never be shipped to it.
--
-- Public read:  POST /rest/v1/rpc/get_certinfo      body {}
-- Public write: POST /rest/v1/rpc/register_certification
--               body {"payload": {...}, "hwid": "...", "fingerprint": "..."}
--
-- The table stores only hashes of IP/HWID/fingerprint. The private UUID and
-- private key stay inside the AES-256-GCM ciphertext; app_id is a separate
-- public UUID used by /certinfo.

create schema if not exists extensions;
create extension if not exists pgcrypto with schema extensions;

create table if not exists public.certifications (
    app_id uuid primary key,
    app text not null,
    uuid_commitment text not null,
    encrypted_key text not null,
    public_key text not null,
    key_fingerprint text not null,
    salt text not null,
    iv text not null,
    iterations integer not null default 210000,
    status text not null default 'SEALED',
    verified boolean not null default true,
    algo text not null default 'AES-256-GCM/PBKDF2-SHA256/ECDSA-P-256',
    created timestamptz not null default now(),
    identity_hash text not null,
    ip_hash text,
    hwid_hash text not null,
    fingerprint_hash text not null,
    constraint certifications_app_len check (char_length(app) between 1 and 64),
    constraint certifications_commitment_fmt check (uuid_commitment ~ '^[0-9a-f]{64}$'),
    constraint certifications_fingerprint_fmt check (key_fingerprint ~ '^[0-9a-f]{64}$'),
    constraint certifications_iterations_range check (iterations between 100000 and 1000000),
    constraint certifications_status_values check (status in ('SEALED', 'REVOKED'))
);

create unique index if not exists certifications_fingerprint_key
    on public.certifications (key_fingerprint);
create index if not exists certifications_identity_idx
    on public.certifications (identity_hash);
create index if not exists certifications_hwid_idx
    on public.certifications (hwid_hash);
create index if not exists certifications_fingerprint_idx
    on public.certifications (fingerprint_hash);
create index if not exists certifications_ip_idx
    on public.certifications (ip_hash);

alter table public.certifications enable row level security;

-- No policies: anon/authenticated cannot touch the table directly. Only the
-- two security-definer functions below can read or write it.
revoke all on table public.certifications from anon, authenticated;

create or replace function public.get_certinfo()
returns jsonb
language sql
security definer
set search_path = public
as $$
    select coalesce(
        jsonb_agg(
            jsonb_build_object(
                'app_id', app_id,
                'app', app,
                'uuid_commitment', uuid_commitment,
                'encrypted_key', encrypted_key,
                'public_key', public_key,
                'key_fingerprint', key_fingerprint,
                'salt', salt,
                'iv', iv,
                'iterations', iterations,
                'status', status,
                'verified', verified,
                'created', created
            ) order by created desc
        ),
        '[]'::jsonb
    )
    from public.certifications
    where status = 'SEALED';
$$;

create or replace function public.register_certification(
    payload jsonb,
    hwid text,
    fingerprint text
)
returns jsonb
language plpgsql
security definer
set search_path = public, extensions
as $$
declare
    v_app_id uuid;
    v_app text;
    v_uuid_commitment text;
    v_encrypted_key text;
    v_public_key text;
    v_key_fingerprint text;
    v_salt text;
    v_iv text;
    v_iterations integer := 210000;
    v_algo text;
    v_identity_hash text;
    v_ip_hash text;
    v_hwid_hash text;
    v_fingerprint_hash text;
    v_identity_count integer;
    v_count integer;
    v_ip text;
    v_headers jsonb;
    v_fingerprint_ok boolean;
    v_locks text[];
    v_lock text;
    v_created timestamptz := now();
begin
    if payload is null or jsonb_typeof(payload) <> 'object' then
        raise exception 'INVALID_ENTRY';
    end if;

    v_app := btrim(coalesce(payload ->> 'app', ''));
    if char_length(v_app) < 1 or char_length(v_app) > 64 then
        raise exception 'INVALID_APP';
    end if;

    if coalesce(payload ->> 'app_id', '') !~* '^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$' then
        raise exception 'INVALID_APP_ID';
    end if;
    v_app_id := (payload ->> 'app_id')::uuid;

    v_uuid_commitment := lower(coalesce(payload ->> 'uuid_commitment', ''));
    if v_uuid_commitment !~ '^[0-9a-f]{64}$' then
        raise exception 'INVALID_UUID_COMMITMENT';
    end if;

    v_key_fingerprint := lower(coalesce(payload ->> 'key_fingerprint', ''));
    if v_key_fingerprint !~ '^[0-9a-f]{64}$' then
        raise exception 'INVALID_FINGERPRINT';
    end if;

    v_encrypted_key := coalesce(payload ->> 'encrypted_key', payload ->> 'ciphertext', '');
    v_public_key := coalesce(payload ->> 'public_key', '');
    v_salt := coalesce(payload ->> 'salt', '');
    v_iv := coalesce(payload ->> 'iv', '');
    v_algo := coalesce(nullif(payload ->> 'algo', ''), 'AES-256-GCM/PBKDF2-SHA256/ECDSA-P-256');

    if char_length(v_encrypted_key) < 32 then
        raise exception 'INVALID_ENCRYPTED_KEY';
    end if;
    if char_length(v_public_key) < 32 then
        raise exception 'INVALID_PUBLIC_KEY';
    end if;
    if char_length(v_salt) < 8 or char_length(v_iv) < 8 then
        raise exception 'INVALID_ENVELOPE';
    end if;
    if payload ? 'iterations' then
        if (payload ->> 'iterations') !~ '^[0-9]{1,7}$' then
            raise exception 'INVALID_ITERATIONS';
        end if;
        v_iterations := (payload ->> 'iterations')::integer;
    end if;
    if v_iterations < 100000 or v_iterations > 1000000 then
        raise exception 'INVALID_ITERATIONS';
    end if;
    if v_algo <> 'AES-256-GCM/PBKDF2-SHA256/ECDSA-P-256' then
        raise exception 'INVALID_ALGO';
    end if;

    begin
        v_fingerprint_ok := encode(digest(decode(v_public_key, 'base64'), 'sha256'), 'hex') = v_key_fingerprint;
    exception
        when others then
            raise exception 'INVALID_PUBLIC_KEY';
    end;
    if not v_fingerprint_ok then
        raise exception 'FINGERPRINT_MISMATCH';
    end if;

    hwid := btrim(coalesce(hwid, ''));
    fingerprint := btrim(coalesce(fingerprint, ''));
    if char_length(hwid) < 8 or char_length(hwid) > 200 then
        raise exception 'INVALID_HWID';
    end if;
    if char_length(fingerprint) < 16 or char_length(fingerprint) > 200 then
        raise exception 'INVALID_DEVICE_FINGERPRINT';
    end if;

    begin
        v_headers := nullif(current_setting('request.headers', true), '')::jsonb;
    exception
        when others then
            v_headers := '{}'::jsonb;
    end;

    v_ip := btrim(split_part(
        coalesce(v_headers ->> 'x-forwarded-for', v_headers ->> 'x-real-ip', ''),
        ',',
        1
    ));
    if v_ip = '' or lower(v_ip) = 'unknown' then
        v_ip_hash := null;
    else
        v_ip_hash := encode(digest('byrust-ip-v1:' || v_ip, 'sha256'), 'hex');
    end if;

    v_hwid_hash := encode(digest('byrust-hwid-v1:' || hwid, 'sha256'), 'hex');
    v_fingerprint_hash := encode(digest('byrust-fingerprint-v1:' || fingerprint, 'sha256'), 'hex');
    v_identity_hash := encode(digest(
        'byrust-identity-v1:' || coalesce(v_ip, 'no-ip') || ':' || hwid || ':' || fingerprint,
        'sha256'
    ), 'hex');

    -- Serialize the limit checks for the same identity, device, fingerprint,
    -- and IP. Lock order is sorted to avoid deadlocks.
    v_locks := array[v_identity_hash, v_hwid_hash, v_fingerprint_hash];
    if v_ip_hash is not null then
        v_locks := array_append(v_locks, v_ip_hash);
    end if;
    select array_agg(value order by value) into v_locks from unnest(v_locks) as value;
    foreach v_lock in array v_locks loop
        perform pg_advisory_xact_lock(hashtextextended(v_lock, 0));
    end loop;

    select count(*) into v_identity_count
        from public.certifications
        where identity_hash = v_identity_hash;
    if v_identity_count >= 2 then
        raise exception 'LIMIT_REACHED_IDENTITY';
    end if;

    select count(*) into v_count
        from public.certifications
        where hwid_hash = v_hwid_hash;
    if v_count >= 2 then
        raise exception 'LIMIT_REACHED_HWID';
    end if;

    select count(*) into v_count
        from public.certifications
        where fingerprint_hash = v_fingerprint_hash;
    if v_count >= 2 then
        raise exception 'LIMIT_REACHED_FINGERPRINT';
    end if;

    if v_ip_hash is not null then
        select count(*) into v_count
            from public.certifications
            where ip_hash = v_ip_hash;
        if v_count >= 2 then
            raise exception 'LIMIT_REACHED_IP';
        end if;
    end if;

    insert into public.certifications (
        app_id, app, uuid_commitment, encrypted_key, public_key,
        key_fingerprint, salt, iv, iterations, status, verified, algo,
        created, identity_hash, ip_hash, hwid_hash, fingerprint_hash
    ) values (
        v_app_id, v_app, v_uuid_commitment, v_encrypted_key, v_public_key,
        v_key_fingerprint, v_salt, v_iv, v_iterations, 'SEALED', true, v_algo,
        v_created, v_identity_hash, v_ip_hash, v_hwid_hash, v_fingerprint_hash
    );

    return jsonb_build_object(
        'app_id', v_app_id,
        'app', v_app,
        'uuid_commitment', v_uuid_commitment,
        'encrypted_key', v_encrypted_key,
        'public_key', v_public_key,
        'key_fingerprint', v_key_fingerprint,
        'salt', v_salt,
        'iv', v_iv,
        'iterations', v_iterations,
        'status', 'SEALED',
        'verified', true,
        'created', v_created
    );
exception
    when unique_violation then
        raise exception 'DUPLICATE_ENTRY';
end;
$$;

revoke all on function public.get_certinfo() from public;
revoke all on function public.register_certification(jsonb, text, text) from public;
grant execute on function public.get_certinfo() to anon, authenticated;
grant execute on function public.register_certification(jsonb, text, text) to anon, authenticated;

comment on table public.certifications is
    'RustWeb public certification registry. Private UUID/private key stay inside encrypted_key; app_id is public.';
comment on function public.get_certinfo() is
    'Returns public certification entries without identity hashes.';
comment on function public.register_certification(jsonb, text, text) is
    'Registers one app, max 2 per identity/HWID/fingerprint/IP. Client supplies encrypted entry and device identifiers; raw values are hashed.';

notify pgrst, 'reload schema';
