/// Simple threshold-based impact detector.
/// Classifies hits into three tiers based on acceleration magnitude.

#[derive(Debug, Clone, Copy)]
pub enum Severity {
    Light,  // 1 undo
    Medium, // 3 undos
    Hard,   // 5 undos
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Light => "light",
            Severity::Medium => "medium",
            Severity::Hard => "hard",
        }
    }

    pub fn undos(&self) -> u32 {
        match self {
            Severity::Light => 1,
            Severity::Medium => 3,
            Severity::Hard => 5,
        }
    }
}

pub struct HitEvent {
    pub severity: Severity,
    pub amplitude: f64,
}

pub struct Detector {
    baseline: f64,
    samples_seen: u64,
    cooldown_remaining: u32,
}

impl Detector {
    pub fn new() -> Self {
        Self {
            baseline: 1.0, // ~1g at rest (gravity)
            samples_seen: 0,
            cooldown_remaining: 0,
        }
    }

    /// Process a single accelerometer sample (x, y, z in g-force).
    /// Returns a HitEvent if an impact is detected.
    pub fn process(&mut self, x: f64, y: f64, z: f64) -> Option<HitEvent> {
        let mag = (x * x + y * y + z * z).sqrt();

        // Calibration period: let the baseline settle
        if self.samples_seen < 100 {
            self.baseline = self.baseline * 0.9 + mag * 0.1;
            self.samples_seen += 1;
            return None;
        }
        self.samples_seen += 1;

        // Cooldown: ignore samples after a recent hit to avoid multi-triggering
        if self.cooldown_remaining > 0 {
            self.cooldown_remaining -= 1;
            // Still update baseline slowly during cooldown
            self.baseline = self.baseline * 0.999 + mag * 0.001;
            return None;
        }

        let excess = mag - self.baseline;

        // Update baseline slowly (tracks drift but not impacts)
        self.baseline = self.baseline * 0.999 + mag * 0.001;

        // Thresholds (in g above baseline)
        let result = if excess > 2.0 {
            Some(HitEvent {
                severity: Severity::Hard,
                amplitude: excess,
            })
        } else if excess > 1.0 {
            Some(HitEvent {
                severity: Severity::Medium,
                amplitude: excess,
            })
        } else if excess > 0.3 {
            Some(HitEvent {
                severity: Severity::Light,
                amplitude: excess,
            })
        } else {
            None
        };

        if result.is_some() {
            // ~500ms cooldown at ~100Hz sample rate
            self.cooldown_remaining = 50;
        }

        result
    }
}
