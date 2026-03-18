ALTER TABLE schedules
  ADD COLUMN delivery_thread_not_found_streak INTEGER NOT NULL DEFAULT 0;
