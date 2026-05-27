//! Model price table. USD per **token** (not per MTok). Source:
//! claude.com/pricing, snapshot embedded in repo. Schema v1.

#[derive(Debug, Clone, Copy)]
pub struct ModelPrice {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write_5m: f64,
    pub cache_write_1h: f64,
}

const fn per_mtok(usd_per_mtok: f64) -> f64 {
    usd_per_mtok / 1_000_000.0
}

const OPUS: ModelPrice = ModelPrice {
    input: per_mtok(5.0),
    output: per_mtok(25.0),
    cache_read: per_mtok(0.50),
    cache_write_5m: per_mtok(6.25),
    cache_write_1h: per_mtok(10.0),
};

const OPUS_LEGACY: ModelPrice = ModelPrice {
    input: per_mtok(15.0),
    output: per_mtok(75.0),
    cache_read: per_mtok(1.50),
    cache_write_5m: per_mtok(18.75),
    cache_write_1h: per_mtok(30.0),
};

const SONNET: ModelPrice = ModelPrice {
    input: per_mtok(3.0),
    output: per_mtok(15.0),
    cache_read: per_mtok(0.30),
    cache_write_5m: per_mtok(3.75),
    cache_write_1h: per_mtok(6.0),
};

const HAIKU_45: ModelPrice = ModelPrice {
    input: per_mtok(1.0),
    output: per_mtok(5.0),
    cache_read: per_mtok(0.10),
    cache_write_5m: per_mtok(1.25),
    cache_write_1h: per_mtok(2.0),
};

const HAIKU_35: ModelPrice = ModelPrice {
    input: per_mtok(0.80),
    output: per_mtok(4.0),
    cache_read: per_mtok(0.08),
    cache_write_5m: per_mtok(1.0),
    cache_write_1h: per_mtok(1.60),
};

/// Pick the price tier from a model id. Unknown ids fall back to Sonnet — the
/// most common middle-tier so we under-report Opus but don't over-report Haiku.
/// Logged once per unknown id at the call site.
pub fn model_price(model: &str) -> ModelPrice {
    let m = model.to_ascii_lowercase();
    if m.contains("opus-4-7") || m.contains("opus-4-6") || m.contains("opus-4-5") {
        OPUS
    } else if m.contains("opus-4-1") || m.contains("opus-4") {
        OPUS_LEGACY
    } else if m.contains("sonnet") {
        SONNET
    } else if m.contains("haiku-4") {
        HAIKU_45
    } else if m.contains("haiku-3") {
        HAIKU_35
    } else {
        SONNET
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opus_47_matches_official_rates() {
        let p = model_price("claude-opus-4-7");
        assert!((p.input - 5e-6).abs() < 1e-12);
        assert!((p.output - 25e-6).abs() < 1e-12);
    }

    #[test]
    fn haiku_45_matches_official_rates() {
        let p = model_price("claude-haiku-4-5-20251001");
        assert!((p.input - 1e-6).abs() < 1e-12);
        assert!((p.cache_read - 0.1e-6).abs() < 1e-12);
    }

    #[test]
    fn unknown_falls_back_to_sonnet() {
        let p = model_price("some-future-model");
        assert!((p.input - 3e-6).abs() < 1e-12);
    }
}
