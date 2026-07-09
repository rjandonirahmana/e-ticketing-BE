-- ============================================================================
-- Migration: 011_system_user.sql  —  System user untuk pesan sistem grup chat
-- ============================================================================
-- BUG: group_messages.sender_id REFERENCES users(id) NOT NULL. Pesan sistem
-- ("X bergabung ke grup") memakai sender_id = ULID nol (16 byte 0x00), tapi
-- user placeholder itu TIDAK PERNAH dibuat (INSERT-nya di-comment di 002).
-- Akibatnya setiap save_message() sistem kena FK violation → join_room() balas
-- error walau member sudah tersimpan, dan auto_join_after_payment() gagal.
--
-- Fix: buat user placeholder dengan id 16 byte nol (sama dengan hasil decode
-- ULID "00000000000000000000000000" yang dipakai build_system_msg()).
--
--   psql "$DATABASE_URL" -f migration/011_system_user.sql
-- ============================================================================

INSERT INTO users (id, email, password_hash, name, phone, role)
VALUES (
    decode('00000000000000000000000000000000', 'hex'),  -- 16 byte 0x00
    NULL,
    NULL,
    'System Pulse',
    '62-system',                                          -- phone WAJIB non-null & unik (migrasi 012)
    'admin'
)
ON CONFLICT (id) DO NOTHING;
