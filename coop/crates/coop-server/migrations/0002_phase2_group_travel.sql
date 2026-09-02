-- Additive Phase 4 group-travel schema.  Migration 0001 remains unchanged.

ALTER TABLE phase2_characters
    ADD COLUMN IF NOT EXISTS world_revision bigint NOT NULL DEFAULT 0
    CHECK (world_revision BETWEEN 0 AND 9223372036854775807);

CREATE TABLE phase2_groups (
    group_id uuid PRIMARY KEY,
    member_a uuid NOT NULL REFERENCES phase2_characters(character_id),
    member_b uuid NOT NULL REFERENCES phase2_characters(character_id),
    region text NOT NULL CHECK (region IN ('HOENN', 'KANTO', 'JOHTO', 'SEVII')),
    map text NOT NULL CHECK (
        length(map) BETWEEN 1 AND 128
        AND map = upper(map)
        AND map !~ '[^A-Z0-9_]'
    ),
    channel integer NOT NULL CHECK (channel BETWEEN 0 AND 65535),
    status text NOT NULL DEFAULT 'ACTIVE'
        CHECK (status IN ('ACTIVE', 'CLOSED')),
    created_at timestamptz NOT NULL DEFAULT now(),
    closed_at timestamptz,
    CHECK (member_a <> member_b),
    CHECK (member_a < member_b),
    CHECK ((status = 'ACTIVE' AND closed_at IS NULL)
        OR (status = 'CLOSED' AND closed_at IS NOT NULL))
);

CREATE TABLE phase2_group_members (
    group_id uuid NOT NULL REFERENCES phase2_groups(group_id) ON DELETE CASCADE,
    character_id uuid NOT NULL REFERENCES phase2_characters(character_id),
    member_slot smallint NOT NULL CHECK (member_slot IN (0, 1)),
    active boolean NOT NULL DEFAULT true,
    joined_at timestamptz NOT NULL DEFAULT now(),
    left_at timestamptz,
    PRIMARY KEY (group_id, character_id),
    UNIQUE (group_id, member_slot),
    CHECK ((active AND left_at IS NULL) OR (NOT active AND left_at IS NOT NULL))
);

CREATE UNIQUE INDEX phase2_one_active_group_per_character
    ON phase2_group_members(character_id) WHERE active;

CREATE TABLE phase2_group_invitations (
    invitation_id uuid PRIMARY KEY,
    inviter_character_id uuid NOT NULL REFERENCES phase2_characters(character_id),
    invitee_character_id uuid NOT NULL REFERENCES phase2_characters(character_id),
    status text NOT NULL DEFAULT 'PENDING'
        CHECK (status IN ('PENDING', 'ACCEPTED', 'EXPIRED', 'CANCELLED')),
    created_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    accepted_at timestamptz,
    CHECK (inviter_character_id <> invitee_character_id),
    CHECK (expires_at > created_at),
    CHECK ((status = 'ACCEPTED' AND accepted_at IS NOT NULL)
        OR (status <> 'ACCEPTED' AND accepted_at IS NULL))
);

CREATE INDEX phase2_group_invitation_invitee_idx
    ON phase2_group_invitations(invitee_character_id, status, expires_at);

-- A group is always exactly two active, canonically ordered members.  The
-- trigger is deferred so one transaction can install both rows atomically.
CREATE FUNCTION phase2_validate_group_members() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    member_count integer;
    first_member uuid;
    second_member uuid;
    group_key uuid;
    group_status text;
BEGIN
    group_key := CASE WHEN TG_OP = 'DELETE' THEN OLD.group_id ELSE NEW.group_id END;
    IF TG_OP = 'DELETE' AND NOT EXISTS (
        SELECT 1 FROM phase2_groups WHERE group_id = group_key
    ) THEN
        RETURN OLD;
    END IF;
    SELECT status
      INTO group_status
      FROM phase2_groups
     WHERE group_id = group_key;
    IF group_status IS NULL THEN
        RAISE EXCEPTION 'group row is required before member rows';
    END IF;
    SELECT count(*)
      INTO member_count
      FROM phase2_group_members
     WHERE group_id = group_key AND active;
    IF group_status = 'CLOSED' THEN
        IF member_count <> 0 THEN
            RAISE EXCEPTION 'closed group must not contain active members';
        END IF;
        IF TG_OP = 'DELETE' THEN
            RETURN OLD;
        END IF;
        RETURN NEW;
    END IF;
    IF group_status <> 'ACTIVE' OR member_count <> 2 THEN
        RAISE EXCEPTION 'active group must contain exactly two distinct members';
    END IF;
    SELECT character_id
      INTO first_member
      FROM phase2_group_members
     WHERE group_id = group_key AND active
     ORDER BY character_id
     LIMIT 1;
    SELECT character_id
      INTO second_member
      FROM phase2_group_members
     WHERE group_id = group_key AND active
     ORDER BY character_id
     OFFSET 1 LIMIT 1;
    IF first_member = second_member THEN
        RAISE EXCEPTION 'active group must contain exactly two distinct members';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM phase2_groups g
         WHERE g.group_id = group_key
           AND g.member_a = first_member
           AND g.member_b = second_member
    ) THEN
        RAISE EXCEPTION 'group member rows do not match canonical group actors';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM phase2_group_members
         WHERE group_id = group_key AND active
           AND character_id = first_member AND member_slot = 0
    ) OR NOT EXISTS (
        SELECT 1 FROM phase2_group_members
         WHERE group_id = group_key AND active
           AND character_id = second_member AND member_slot = 1
    ) THEN
        RAISE EXCEPTION 'group member slots do not match canonical actor order';
    END IF;
    RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER phase2_group_members_integrity
AFTER INSERT OR UPDATE OR DELETE ON phase2_group_members
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION phase2_validate_group_members();

CREATE CONSTRAINT TRIGGER phase2_group_integrity
AFTER INSERT OR UPDATE ON phase2_groups
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION phase2_validate_group_members();

CREATE FUNCTION phase2_guard_group_status() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.member_a IS DISTINCT FROM OLD.member_a
       OR NEW.member_b IS DISTINCT FROM OLD.member_b THEN
        RAISE EXCEPTION 'group actors are immutable';
    END IF;
    IF OLD.status = 'CLOSED' AND NEW.status <> 'CLOSED' THEN
        RAISE EXCEPTION 'closed group status cannot be reopened';
    END IF;
    IF OLD.status = 'ACTIVE' AND NEW.status = 'CLOSED'
       AND NEW.closed_at IS NULL THEN
        RAISE EXCEPTION 'closed group requires closed_at';
    END IF;
    IF OLD.status = 'CLOSED' AND (
        NEW.member_a IS DISTINCT FROM OLD.member_a
        OR NEW.member_b IS DISTINCT FROM OLD.member_b
        OR NEW.region IS DISTINCT FROM OLD.region
        OR NEW.map IS DISTINCT FROM OLD.map
        OR NEW.channel IS DISTINCT FROM OLD.channel
    ) THEN
        RAISE EXCEPTION 'closed group identity and zone are immutable';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER phase2_group_status_guard
BEFORE UPDATE ON phase2_groups
FOR EACH ROW EXECUTE FUNCTION phase2_guard_group_status();

-- Groups must follow the terminal lifecycle before their durable row can be
-- removed.  Check membership before ON DELETE CASCADE gets a chance to remove
-- the rows, so a CLOSED group can only be deleted after all members are
-- explicitly inactive.
CREATE FUNCTION phase2_guard_group_delete() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.status = 'ACTIVE' THEN
        RAISE EXCEPTION 'active group must be closed before deletion';
    END IF;
    IF EXISTS (
        SELECT 1
          FROM phase2_group_members
         WHERE group_id = OLD.group_id AND active
    ) THEN
        RAISE EXCEPTION 'closed group must not contain active members';
    END IF;
    RETURN OLD;
END;
$$;

CREATE TRIGGER phase2_group_delete_guard
BEFORE DELETE ON phase2_groups
FOR EACH ROW EXECUTE FUNCTION phase2_guard_group_delete();

CREATE FUNCTION phase2_guard_group_invitation() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.inviter_character_id IS DISTINCT FROM NEW.inviter_character_id
       OR OLD.invitee_character_id IS DISTINCT FROM NEW.invitee_character_id
       OR OLD.created_at IS DISTINCT FROM NEW.created_at
       OR OLD.expires_at IS DISTINCT FROM NEW.expires_at THEN
        RAISE EXCEPTION 'group invitation actors and deadline are immutable';
    END IF;
    IF OLD.status <> 'PENDING' AND NEW.status <> OLD.status THEN
        RAISE EXCEPTION 'terminal invitation status cannot transition';
    END IF;
    IF OLD.status = 'PENDING' AND NEW.status = 'ACCEPTED'
       AND NEW.accepted_at IS NULL THEN
        RAISE EXCEPTION 'accepted invitation requires accepted_at';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER phase2_group_invitation_guard
BEFORE UPDATE ON phase2_group_invitations
FOR EACH ROW EXECUTE FUNCTION phase2_guard_group_invitation();

CREATE INDEX phase2_group_member_group_idx
    ON phase2_group_members(group_id, active);
