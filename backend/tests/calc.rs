use serde::Deserialize;
use vresto::money::*;

#[derive(Deserialize)]
struct Case {
    name: String,
    billing: Billing,
    subtotal: i64,
    order_type: String,
    discount: Discount,
    shipping_fee: i64,
    points_used: i64,
    expected: Totals,
}

#[test]
fn matches_reference_vectors() {
    let cases: Vec<Case> = serde_json::from_str(include_str!("../testdata/calc_vectors.json")).unwrap();
    assert!(cases.len() > 400);
    let (mut bad, mut ties) = (vec![], 0);
    for c in &cases {
        let ot = OrderType::parse(&c.order_type).unwrap();
        let got = calculate(&c.billing, c.subtotal, ot, c.discount, c.shipping_fee, c.points_used);
        if got == c.expected { continue; }

        let exact_tie = matches!(c.discount, Discount::Percent { value }
            if (c.subtotal as i128 * (value.clamp(0.0, 100.0) * 100.0).round() as i128) % 10_000 == 5_000);
        if exact_tie && got.discount_amount == c.expected.discount_amount + 1 { ties += 1; } else {
            bad.push(format!("{}: got {:?} expected {:?}", c.name, got, c.expected));
        }
    }
    assert!(bad.is_empty(), "{} dari {} vektor berbeda:\n{}", bad.len(), cases.len(), bad.iter().take(5).cloned().collect::<Vec<_>>().join("\n"));
    assert!(ties <= 8, "terlalu banyak selisih tie ({ties}) — kemungkinan bug, bukan galat float");
    eprintln!("{} vektor cocok persis, {} selisih Rp1 karena galat float di referensi (tie .5 murni)", cases.len() - ties, ties);
}

#[test]
fn order_type_aliases_are_normalised() {

    for s in ["take_away", "takeaway", "Take Away", "take-away", "TAKEAWAY"] {
        assert_eq!(OrderType::parse(s), Some(OrderType::TakeAway), "{s}");
    }
    assert_eq!(OrderType::parse("Dine In"), Some(OrderType::DineIn));
    assert_eq!(OrderType::parse("ngawur"), None);
}

#[test]
fn rounding_is_half_up() {
    assert_eq!(apply_bps(15, 1000), 2);
    assert_eq!(apply_bps(14, 1000), 1);
    assert_eq!(apply_bps(12_500, 1100), 1375);
    assert_eq!(rate_to_bps(0.11), 1100);
}
