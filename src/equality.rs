use toml_edit::Item as ItemRs;

/// Semantically compare two toml_edit Datetimes.
///
/// Full aware datetimes (date + time + offset) are normalized to UTC before
/// comparing, so the same instant at different offsets compares equal.
/// Partial datetimes (date-only, time-only, naive) are compared field-by-field.
fn datetime_eq(a: &toml_edit::Datetime, b: &toml_edit::Datetime) -> bool {
    fn time_eq(a: &Option<toml_edit::Time>, b: &Option<toml_edit::Time>) -> bool {
        match (a, b) {
            (Some(a), Some(b)) => {
                a.hour == b.hour
                    && a.minute == b.minute
                    && a.second.unwrap_or(0) == b.second.unwrap_or(0)
                    && a.nanosecond.unwrap_or(0) == b.nanosecond.unwrap_or(0)
            }
            (None, None) => true,
            _ => false,
        }
    }

    // Full aware datetimes: normalize to UTC before comparing.
    if let (Some(ad), Some(at), Some(ao), Some(bd), Some(bt), Some(bo)) =
        (&a.date, &a.time, &a.offset, &b.date, &b.time, &b.offset)
    {
        return utc_minutes(ad, at, ao) == utc_minutes(bd, bt, bo)
            && at.second.unwrap_or(0) == bt.second.unwrap_or(0)
            && at.nanosecond.unwrap_or(0) == bt.nanosecond.unwrap_or(0);
    }

    // Partial datetimes (date-only, time-only, naive): field-by-field.
    a.date == b.date && time_eq(&a.time, &b.time) && a.offset == b.offset
}

/// Total UTC minutes for an aware datetime (Hinnant's civil-day algorithm).
fn utc_minutes(date: &toml_edit::Date, time: &toml_edit::Time, offset: &toml_edit::Offset) -> i64 {
    let off = match offset {
        toml_edit::Offset::Z => 0i64,
        toml_edit::Offset::Custom { minutes } => i64::from(*minutes),
    };
    let days = days_from_civil(
        i64::from(date.year),
        i64::from(date.month),
        i64::from(date.day),
    );
    days * 24 * 60 + i64::from(time.hour) * 60 + i64::from(time.minute) - off
}

/// Deterministic day count from a civil (year, month, day) triple.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let m = if m <= 2 { m + 9 } else { m - 3 };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * m + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe
}

/// Check whether an integer and a float represent the same numeric value,
/// mirroring Python's cross-type `int == float` semantics.
fn int_float_eq(i: i64, f: f64) -> bool {
    // Fast reject: non-finite floats are never equal to an integer.
    // Then check that the float is a whole number and round-trips exactly.
    f.is_finite() && f == (i as f64) && (f as i64) == i
}

/// Compare two toml_edit Values structurally (pure Rust, no Python allocation).
fn values_structural_eq(a: &toml_edit::Value, b: &toml_edit::Value) -> bool {
    match (a, b) {
        (toml_edit::Value::String(a), toml_edit::Value::String(b)) => a.value() == b.value(),
        (toml_edit::Value::Integer(a), toml_edit::Value::Integer(b)) => a.value() == b.value(),
        (toml_edit::Value::Float(a), toml_edit::Value::Float(b)) => a.value() == b.value(),
        (toml_edit::Value::Integer(i), toml_edit::Value::Float(f))
        | (toml_edit::Value::Float(f), toml_edit::Value::Integer(i)) => {
            int_float_eq(*i.value(), *f.value())
        }
        (toml_edit::Value::Boolean(a), toml_edit::Value::Boolean(b)) => a.value() == b.value(),
        (toml_edit::Value::Datetime(a), toml_edit::Value::Datetime(b)) => {
            datetime_eq(a.value(), b.value())
        }
        (toml_edit::Value::Array(a), toml_edit::Value::Array(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b.iter())
                    .all(|(va, vb)| values_structural_eq(va, vb))
        }
        (toml_edit::Value::InlineTable(a), toml_edit::Value::InlineTable(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.get(k).is_some_and(|bv| values_structural_eq(v, bv)))
        }
        _ => false,
    }
}

fn tables_structural_eq(a: &toml_edit::Table, b: &toml_edit::Table) -> bool {
    a.len() == b.len()
        && a.iter()
            .all(|(k, v)| b.get(k).is_some_and(|bv| items_structural_eq(v, bv)))
}

/// Compare a Table with an InlineTable by walking their entries directly.
fn table_inline_eq(table: &toml_edit::Table, it: &toml_edit::InlineTable) -> bool {
    table.len() == it.len()
        && table
            .iter()
            .all(|(k, item)| it.get(k).is_some_and(|v| item_value_eq(item, v)))
}

/// Compare an Item with a Value across the Table/InlineTable and AoT/Array
/// boundaries.
pub(crate) fn item_value_eq(item: &ItemRs, value: &toml_edit::Value) -> bool {
    match item {
        ItemRs::Value(v) => values_structural_eq(v, value),
        ItemRs::Table(t) => {
            matches!(value, toml_edit::Value::InlineTable(it) if table_inline_eq(t, it))
        }
        ItemRs::ArrayOfTables(aot) => {
            matches!(value, toml_edit::Value::Array(arr) if aot_array_eq(aot, arr))
        }
        _ => false,
    }
}

/// Compare an AoT with an Array of inline tables directly.
fn aot_array_eq(aot: &toml_edit::ArrayOfTables, arr: &toml_edit::Array) -> bool {
    aot.len() == arr.len()
        && aot
            .iter()
            .zip(arr.iter())
            .all(|(t, v)| matches!(v, toml_edit::Value::InlineTable(it) if table_inline_eq(t, it)))
}

/// Compare an Item with a Table across the Table/InlineTable boundary.
pub(crate) fn item_table_eq(item: &ItemRs, table: &toml_edit::Table) -> bool {
    match item {
        ItemRs::Table(t) => tables_structural_eq(t, table),
        ItemRs::Value(toml_edit::Value::InlineTable(it)) => table_inline_eq(table, it),
        _ => false,
    }
}

pub(crate) fn items_structural_eq(a: &ItemRs, b: &ItemRs) -> bool {
    match b {
        ItemRs::Value(v) => item_value_eq(a, v),
        ItemRs::Table(t) => item_table_eq(a, t),
        ItemRs::ArrayOfTables(ab) => match a {
            ItemRs::ArrayOfTables(aa) => {
                aa.len() == ab.len()
                    && aa
                        .iter()
                        .zip(ab.iter())
                        .all(|(ta, tb)| tables_structural_eq(ta, tb))
            }
            ItemRs::Value(toml_edit::Value::Array(arr)) => aot_array_eq(ab, arr),
            _ => false,
        },
        _ => false,
    }
}
