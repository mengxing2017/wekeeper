-- Re-evaluate current eligible messages with the new matching algorithm.
-- Already rejected versions are excluded by the recheck worker.
INSERT INTO keyword_recheck_batches(id) VALUES(gen_random_uuid());
