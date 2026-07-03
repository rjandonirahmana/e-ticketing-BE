-- ============================================================================
-- Migration: 008_user_affinity.sql — Rekomendasi berbasis perilaku (server-side)
-- ============================================================================
-- Menyimpan "afinitas" user terhadap kategori event berdasarkan perilaku
-- (event yang dibuka/dicari). Setiap kali user login membuka detail event,
-- skor kategori event itu di-upsert (+1). Rekomendasi "Untuk Kamu" mengambil
-- kategori berskor tertinggi lalu menampilkan event kategori tsb.
--
-- Kenapa tabel agregat (bukan log mentah)? Upsert O(1) per view + baca cepat
-- (top-N per user) → ringan untuk VPS kecil. Recency dijaga lewat updated_at.
-- Anonim (belum login) tetap ditangani localStorage di sisi client.
-- ============================================================================

CREATE TABLE IF NOT EXISTS user_affinity (
    user_id    bytea            NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    category   text             NOT NULL,
    score      double precision NOT NULL DEFAULT 0,
    updated_at timestamptz      NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, category)
);

-- Ambil kategori favorit user (ORDER BY score DESC) dengan cepat.
CREATE INDEX IF NOT EXISTS idx_user_affinity_top
    ON user_affinity (user_id, score DESC);
