-- Phase 4: the user's rating of a reply (Feedback JSON: rating, note, createdAt). Stored only
-- locally.
ALTER TABLE messages ADD COLUMN feedback TEXT;
