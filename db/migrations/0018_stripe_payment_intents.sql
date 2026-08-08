ALTER TABLE stripe_webhook_events
    ADD COLUMN payment_intent_id TEXT
    CHECK (payment_intent_id IS NULL OR payment_intent_id ~ '^pi_[A-Za-z0-9_]+$');

CREATE UNIQUE INDEX stripe_webhook_one_recorded_payment_per_intent
    ON stripe_webhook_events (payment_intent_id)
    WHERE outcome = 'payment_recorded' AND payment_intent_id IS NOT NULL;

COMMENT ON COLUMN stripe_webhook_events.payment_intent_id IS
    'Signed Stripe PaymentIntent identity retained for an idempotent operator refund.';
