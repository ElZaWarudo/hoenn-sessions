-- Phase 2 PostgreSQL contract.  Passwords, invitations and bearer values are
-- always stored as server-side hashes; this migration never stores plaintext.
CREATE EXTENSION IF NOT EXISTS citext;

CREATE TABLE phase2_users (
    user_id uuid PRIMARY KEY,
    username citext NOT NULL UNIQUE,
    password_phc text NOT NULL,
    disabled boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE phase2_invitations (
    invitation_sha256 bytea PRIMARY KEY CHECK (octet_length(invitation_sha256) = 32),
    consumed_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE phase2_characters (
    character_id uuid PRIMARY KEY,
    user_id uuid NOT NULL UNIQUE REFERENCES phase2_users(user_id),
    region text NOT NULL CHECK (region IN ('HOENN', 'KANTO', 'JOHTO', 'SEVII')),
    current_revision bigint NOT NULL DEFAULT 0 CHECK (current_revision BETWEEN 0 AND 9223372036854775807),
    active_snapshot_id uuid,
    last_session_epoch bigint NOT NULL DEFAULT 0 CHECK (last_session_epoch BETWEEN 0 AND 4294967295),
    state_json jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (character_id, user_id)
);

CREATE TABLE phase2_refresh_families (
    family_id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES phase2_users(user_id),
    character_id uuid NOT NULL REFERENCES phase2_characters(character_id),
    expires_at timestamptz NOT NULL,
    revoked_at timestamptz,
    UNIQUE (family_id, user_id, character_id),
    FOREIGN KEY (character_id, user_id)
        REFERENCES phase2_characters(character_id, user_id)
);

CREATE TABLE phase2_refresh_generations (
    token_sha256 bytea PRIMARY KEY CHECK (octet_length(token_sha256) = 32),
    family_id uuid NOT NULL REFERENCES phase2_refresh_families(family_id),
    user_id uuid NOT NULL REFERENCES phase2_users(user_id),
    character_id uuid NOT NULL REFERENCES phase2_characters(character_id),
    generation bigint NOT NULL CHECK (generation BETWEEN 0 AND 4294967295),
    expires_at timestamptz NOT NULL,
    consumed_at timestamptz,
    UNIQUE (family_id, generation),
    FOREIGN KEY (family_id, user_id, character_id)
        REFERENCES phase2_refresh_families(family_id, user_id, character_id),
    FOREIGN KEY (character_id, user_id)
        REFERENCES phase2_characters(character_id, user_id)
);

CREATE TABLE phase2_access_tokens (
    token_sha256 bytea PRIMARY KEY CHECK (octet_length(token_sha256) = 32),
    user_id uuid NOT NULL REFERENCES phase2_users(user_id),
    character_id uuid NOT NULL REFERENCES phase2_characters(character_id),
    family_id uuid NOT NULL REFERENCES phase2_refresh_families(family_id),
    expires_at timestamptz NOT NULL,
    revoked_at timestamptz,
    FOREIGN KEY (character_id, user_id)
        REFERENCES phase2_characters(character_id, user_id),
    FOREIGN KEY (family_id, user_id, character_id)
        REFERENCES phase2_refresh_families(family_id, user_id, character_id)
);

CREATE TABLE phase2_leases (
    character_id uuid PRIMARY KEY REFERENCES phase2_characters(character_id),
    user_id uuid NOT NULL REFERENCES phase2_users(user_id),
    session_id uuid NOT NULL UNIQUE,
    client_instance_id uuid NOT NULL,
    session_epoch bigint NOT NULL CHECK (session_epoch BETWEEN 1 AND 4294967295),
    current_revision bigint NOT NULL CHECK (current_revision BETWEEN 0 AND 9223372036854775807),
    expires_at timestamptz NOT NULL,
    reconnect_until timestamptz NOT NULL,
    released_at timestamptz,
    UNIQUE (character_id, session_epoch),
    FOREIGN KEY (character_id, user_id)
        REFERENCES phase2_characters(character_id, user_id)
);

CREATE TABLE phase2_snapshots (
    snapshot_id uuid PRIMARY KEY,
    character_id uuid NOT NULL REFERENCES phase2_characters(character_id),
    user_id uuid NOT NULL REFERENCES phase2_users(user_id),
    session_id uuid NOT NULL,
    session_epoch bigint NOT NULL CHECK (session_epoch BETWEEN 1 AND 4294967295),
    parent_revision bigint NOT NULL CHECK (parent_revision >= 0),
    revision bigint NOT NULL CHECK (revision > 0),
    pending_commits_sha256 bytea NOT NULL CHECK (octet_length(pending_commits_sha256) = 32),
    last_applied_commit uuid,
    status text NOT NULL CHECK (status IN ('PREPARED', 'FINALIZED')),
    created_at timestamptz NOT NULL,
    UNIQUE (character_id, revision),
    UNIQUE (snapshot_id, user_id),
    FOREIGN KEY (character_id, user_id)
        REFERENCES phase2_characters(character_id, user_id),
    CHECK (parent_revision < 9223372036854775807),
    CHECK (revision = parent_revision + 1)
);

CREATE TABLE phase2_snapshot_artifacts (
    snapshot_id uuid NOT NULL REFERENCES phase2_snapshots(snapshot_id) ON DELETE CASCADE,
    artifact text NOT NULL CHECK (artifact IN ('character.sav', 'pending_commits.json', 'resume.ss1')),
    object_key text NOT NULL
        CHECK (length(object_key) BETWEEN 40 AND 512)
        CHECK (object_key !~ '(^|/)\.\.?(/|$)')
        CHECK (object_key ~ '^characters/[0-9a-fA-F-]{36}/snapshots/[0-9a-fA-F-]{36}/(character\.sav|pending_commits\.json|resume\.ss1)$'),
    sha256 bytea NOT NULL CHECK (octet_length(sha256) = 32),
    size_bytes bigint NOT NULL CHECK (
        (artifact = 'character.sav' AND size_bytes IN (131072, 131088))
        OR (artifact = 'pending_commits.json' AND size_bytes BETWEEN 0 AND 1048576)
        OR (artifact = 'resume.ss1' AND size_bytes BETWEEN 1 AND 33554432)
    ),
    artifact_status text NOT NULL DEFAULT 'PREPARED'
        CHECK (artifact_status IN ('PREPARED', 'UPLOADED', 'VERIFIED')),
    uploaded_at timestamptz,
    PRIMARY KEY (snapshot_id, artifact)
);

ALTER TABLE phase2_characters
    ADD CONSTRAINT phase2_characters_active_snapshot_fk
    FOREIGN KEY (active_snapshot_id, user_id)
        REFERENCES phase2_snapshots(snapshot_id, user_id);

CREATE TABLE phase2_idempotency (
    user_id uuid NOT NULL REFERENCES phase2_users(user_id),
    character_id uuid NOT NULL REFERENCES phase2_characters(character_id),
    operation text NOT NULL,
    idempotency_key uuid NOT NULL,
    request_fingerprint bytea NOT NULL CHECK (octet_length(request_fingerprint) = 32),
    response_json jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (character_id, operation, idempotency_key),
    FOREIGN KEY (character_id, user_id)
        REFERENCES phase2_characters(character_id, user_id)
);

-- A finalized snapshot must have both fixed mandatory artifacts, and each
-- mandatory artifact must have reached object-store verification before the
-- transaction can commit. Optional resume state is allowed to be absent.
CREATE FUNCTION phase2_require_snapshot_artifacts() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.status = 'FINALIZED' AND (
        NOT EXISTS (
            SELECT 1 FROM phase2_snapshot_artifacts
            WHERE snapshot_id = NEW.snapshot_id
              AND artifact = 'character.sav'
              AND artifact_status = 'VERIFIED'
        )
        OR NOT EXISTS (
            SELECT 1 FROM phase2_snapshot_artifacts
            WHERE snapshot_id = NEW.snapshot_id
              AND artifact = 'pending_commits.json'
              AND artifact_status = 'VERIFIED'
        )
    ) THEN
        RAISE EXCEPTION 'finalized snapshot is missing mandatory verified artifacts';
    END IF;
    RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER phase2_snapshot_artifacts_complete
AFTER INSERT OR UPDATE ON phase2_snapshots
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION phase2_require_snapshot_artifacts();

-- The active pointer is a tenant- and character-scoped pointer to a
-- finalized snapshot only. A plain foreign key cannot express the status
-- predicate, so enforce it at the transaction boundary.
CREATE FUNCTION phase2_validate_active_snapshot() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.active_snapshot_id IS NOT NULL AND NOT EXISTS (
        SELECT 1
        FROM phase2_snapshots s
        WHERE s.snapshot_id = NEW.active_snapshot_id
          AND s.character_id = NEW.character_id
          AND s.user_id = NEW.user_id
          AND s.status = 'FINALIZED'
    ) THEN
        RAISE EXCEPTION 'active snapshot must be finalized and belong to the character';
    END IF;
    RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER phase2_active_snapshot_integrity
AFTER INSERT OR UPDATE OF active_snapshot_id, character_id, user_id ON phase2_characters
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION phase2_validate_active_snapshot();

-- Invitation consumption is one-way and lease revisions/epochs are bounded
-- to the protocol's unsigned wire representation.
CREATE FUNCTION phase2_guard_invitation_consumption() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.consumed_at IS NOT NULL AND NEW.consumed_at IS NULL THEN
        RAISE EXCEPTION 'invitation consumption cannot be reverted';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER phase2_invitation_consumption_guard
BEFORE UPDATE OF consumed_at ON phase2_invitations
FOR EACH ROW EXECUTE FUNCTION phase2_guard_invitation_consumption();

-- Object metadata and status are immutable once a snapshot is finalized. The
-- complete canonical key is derived from the parent snapshot and artifact as
-- well as the shape check above, preventing cross-character or cross-snapshot
-- object substitution.
CREATE FUNCTION phase2_guard_snapshot_artifact_integrity() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    parent_status text;
    parent_character uuid;
    parent_snapshot uuid;
    expected_key text;
BEGIN
    IF TG_OP = 'INSERT' THEN
        parent_snapshot := NEW.snapshot_id;
    ELSE
        parent_snapshot := OLD.snapshot_id;
    END IF;
    SELECT status, character_id INTO parent_status, parent_character
      FROM phase2_snapshots WHERE snapshot_id = parent_snapshot;
    IF parent_status = 'FINALIZED' THEN
        RAISE EXCEPTION 'finalized snapshot artifacts are immutable';
    END IF;
    IF TG_OP <> 'DELETE' THEN
        IF TG_OP = 'UPDATE' AND (
            NEW.snapshot_id IS DISTINCT FROM OLD.snapshot_id
            OR NEW.artifact IS DISTINCT FROM OLD.artifact
        ) THEN
            RAISE EXCEPTION 'snapshot artifact identity cannot be changed';
        END IF;
        IF TG_OP = 'UPDATE' AND EXISTS (
            SELECT 1 FROM phase2_snapshots
            WHERE snapshot_id = NEW.snapshot_id AND status = 'FINALIZED'
        ) THEN
            RAISE EXCEPTION 'artifact cannot be moved into a finalized snapshot';
        END IF;
        expected_key := format(
            'characters/%s/snapshots/%s/%s',
            parent_character,
            NEW.snapshot_id,
            NEW.artifact
        );
        IF NEW.object_key IS DISTINCT FROM expected_key THEN
            RAISE EXCEPTION 'snapshot artifact object key is not canonical for its parent';
        END IF;
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER phase2_snapshot_artifact_integrity_guard
BEFORE INSERT OR UPDATE OR DELETE ON phase2_snapshot_artifacts
FOR EACH ROW EXECUTE FUNCTION phase2_guard_snapshot_artifact_integrity();

-- A finalized snapshot is an immutable provenance record. In particular,
-- protecting only status would allow a later update to rewrite ownership,
-- revisions, digests, or timestamps while retaining FINALIZED status.
CREATE FUNCTION phase2_guard_finalized_snapshot_immutable() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF OLD.status = 'FINALIZED' THEN
            RAISE EXCEPTION 'finalized snapshot cannot be deleted';
        END IF;
        RETURN OLD;
    END IF;
    IF OLD.status = 'FINALIZED' AND (
        NEW.snapshot_id IS DISTINCT FROM OLD.snapshot_id
        OR NEW.character_id IS DISTINCT FROM OLD.character_id
        OR NEW.user_id IS DISTINCT FROM OLD.user_id
        OR NEW.session_id IS DISTINCT FROM OLD.session_id
        OR NEW.session_epoch IS DISTINCT FROM OLD.session_epoch
        OR NEW.parent_revision IS DISTINCT FROM OLD.parent_revision
        OR NEW.revision IS DISTINCT FROM OLD.revision
        OR NEW.pending_commits_sha256 IS DISTINCT FROM OLD.pending_commits_sha256
        OR NEW.last_applied_commit IS DISTINCT FROM OLD.last_applied_commit
        OR NEW.status IS DISTINCT FROM OLD.status
        OR NEW.created_at IS DISTINCT FROM OLD.created_at
    ) THEN
        RAISE EXCEPTION 'finalized snapshot provenance is immutable';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER phase2_finalized_snapshot_immutable
BEFORE UPDATE OR DELETE ON phase2_snapshots
FOR EACH ROW EXECUTE FUNCTION phase2_guard_finalized_snapshot_immutable();

-- A lease transition may only claim the character revision it fences. This is
-- deferred because finalize/restore advance both rows in one transaction.
CREATE FUNCTION phase2_validate_lease_revision() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM phase2_characters c
        WHERE c.character_id = NEW.character_id
          AND c.user_id = NEW.user_id
          AND c.current_revision = NEW.current_revision
    ) THEN
        RAISE EXCEPTION 'lease revision does not match character revision';
    END IF;
    RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER phase2_lease_revision_integrity
AFTER INSERT OR UPDATE OF current_revision, character_id, user_id ON phase2_leases
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION phase2_validate_lease_revision();

CREATE FUNCTION phase2_guard_snapshot_status() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.status = 'FINALIZED' AND NEW.status <> 'FINALIZED' THEN
        RAISE EXCEPTION 'finalized snapshot status cannot be downgraded';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER phase2_snapshot_status_guard
BEFORE UPDATE OF status ON phase2_snapshots
FOR EACH ROW EXECUTE FUNCTION phase2_guard_snapshot_status();

CREATE INDEX phase2_access_family_idx ON phase2_access_tokens(family_id);
CREATE INDEX phase2_snapshot_character_idx ON phase2_snapshots(character_id, revision DESC);
