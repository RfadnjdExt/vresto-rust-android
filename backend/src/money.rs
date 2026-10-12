use serde::{Deserialize, Serialize};

pub const POINT_VALUE: i64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderType { DineIn, TakeAway, Delivery, Gofood, Grabfood, Shopeefood, Karaoke }

impl OrderType {

    pub fn parse(s: &str) -> Option<Self> {
        let n = s.trim().to_lowercase().replace([' ', '-'], "_");
        Some(match n.as_str() {
            "dine_in" | "dinein" => Self::DineIn,
            "take_away" | "takeaway" => Self::TakeAway,
            "delivery" => Self::Delivery,
            "gofood" => Self::Gofood,
            "grabfood" => Self::Grabfood,
            "shopeefood" => Self::Shopeefood,
            "karaoke" => Self::Karaoke,
            _ => return None,
        })
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DineIn => "dine_in", Self::TakeAway => "take_away", Self::Delivery => "delivery",
            Self::Gofood => "gofood", Self::Grabfood => "grabfood", Self::Shopeefood => "shopeefood", Self::Karaoke => "karaoke",
        }
    }
    pub fn prefix(&self) -> &'static str {
        match self { Self::DineIn => "DI", Self::TakeAway => "TA", Self::Delivery => "DL", _ => "ORD" }
    }
    fn pays_packaging(&self) -> bool { matches!(self, Self::TakeAway | Self::Delivery) }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Billing {
    pub apply_tax: bool,
    pub tax_bps: i64,
    pub apply_service_dine_in: bool,
    pub service_bps: i64,
    pub apply_packaging: bool,
    pub packaging_fee: i64,
}

impl Default for Billing {
    fn default() -> Self {
        Self { apply_tax: true, tax_bps: 1000, apply_service_dine_in: true, service_bps: 500, apply_packaging: true, packaging_fee: 2000 }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Discount {
    None,
    Fixed { value: i64 },

    Percent { value: f64 },
}

impl Discount {

    pub fn from_parts(kind: Option<&str>, value: Option<f64>) -> Self {
        let v = value.unwrap_or(0.0);
        if v <= 0.0 { return Self::None; }
        match kind.map(|k| k.to_lowercase()).as_deref() {
            Some("percent") => Self::Percent { value: v },
            _ => Self::Fixed { value: v.round() as i64 },
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Totals {
    pub subtotal: i64,
    pub discount_amount: i64,
    pub tax_amount: i64,
    pub service_charge: i64,
    pub packaging_fee: i64,
    pub shipping_fee: i64,
    pub points_discount: i64,
    pub total: i64,
}

pub fn apply_bps(amount: i64, bps: i64) -> i64 {
    (((amount as i128) * (bps as i128) + 5_000) / 10_000) as i64
}

pub fn rate_to_bps(rate: f64) -> i64 { (rate * 10_000.0).round() as i64 }

pub fn calculate(b: &Billing, subtotal: i64, ot: OrderType, discount: Discount, shipping_fee: i64, points_used: i64) -> Totals {
    let subtotal = subtotal.max(0);
    let discount_amount = match discount {
        Discount::None => 0,
        Discount::Fixed { value } => value.max(0),
        Discount::Percent { value } => {
            let hundredths = (value.clamp(0.0, 100.0) * 100.0).round() as i64;
            ((subtotal as i128 * hundredths as i128 + 5_000) / 10_000) as i64
        }
    }
    .min(subtotal);
    let base = subtotal - discount_amount;
    let tax_amount = if b.apply_tax { apply_bps(base, b.tax_bps) } else { 0 };
    let service_charge = if ot == OrderType::DineIn && b.apply_service_dine_in { apply_bps(base, b.service_bps) } else { 0 };
    let packaging_fee = if ot.pays_packaging() && b.apply_packaging { b.packaging_fee } else { 0 };
    let shipping_fee = shipping_fee.max(0);
    let points_discount = points_used.max(0) * POINT_VALUE;
    let total = (subtotal - discount_amount + tax_amount + service_charge + packaging_fee + shipping_fee - points_discount).max(0);
    Totals { subtotal, discount_amount, tax_amount, service_charge, packaging_fee, shipping_fee, points_discount, total }
}
