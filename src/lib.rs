uniffi::setup_scaffolding!();

use good_lp::{
    constraint, variable, variables, Expression, IntoAffineExpression, Solution, SolverModel,
};

/// One category's envelope, as seen by the solver. All amounts are in the
/// app's base currency and already converted by the Swift side — this crate
/// never touches currency conversion or SwiftData.
#[derive(uniffi::Record, Clone)]
pub struct EnvelopeInput {
    /// `EnvelopeRecord.uuid.uuidString`, round-tripped so the Swift side can
    /// match results back to envelopes without relying on array order.
    pub id: String,
    /// Average monthly spend for this envelope's category over the lookback
    /// window. The solver treats this as the ideal allocation and only
    /// deviates from it when `min_amount`/`max_amount` force it to.
    pub historical_average: f64,
    /// Hard floor (e.g. a rollover deficit that must be covered, or 0).
    pub min_amount: f64,
    /// Hard ceiling (e.g. a user-set cap, or a large sentinel for "no cap").
    pub max_amount: f64,
    /// Relative importance: higher priority envelopes are pulled closer to
    /// their historical average first when money is tight.
    pub priority: f64,
}

/// The solver's suggestion for one envelope. `id` matches `EnvelopeInput.id`.
#[derive(uniffi::Record, Debug)]
pub struct AllocationResult {
    pub id: String,
    pub suggested_amount: f64,
}

/// Outcome of one `suggest_allocation` call — the suggestions plus whether
/// the inputs had to be relaxed to produce a feasible plan.
#[derive(uniffi::Record, Debug)]
pub struct AllocationPlan {
    pub results: Vec<AllocationResult>,
    /// `true` when the requested minimums didn't fit inside `available` and
    /// were scaled down proportionally so a plan could still be produced —
    /// the Swift side should tell the user their floors couldn't be fully
    /// honored instead of presenting the numbers as an exact match.
    pub minimums_were_relaxed: bool,
}

#[derive(uniffi::Error, thiserror::Error, Debug)]
pub enum AllocationError {
    #[error("at least one envelope is required")]
    NoEnvelopes,
    #[error("available amount must not be negative")]
    NegativeAvailable,
    #[error("envelope {id} has min_amount greater than max_amount")]
    InvalidBounds { id: String },
    #[error("no allocation satisfies the given constraints")]
    Infeasible,
}

/// Suggests how to split `available` across `envelopes`.
///
/// Modeled as goal programming: minimize the priority-weighted absolute
/// distance between each envelope's suggested amount and its historical
/// average, subject to `min_amount <= x_i <= max_amount` and
/// `sum(x_i) <= available`. The inequality (rather than an exact split) is
/// deliberate — once every envelope reaches its target the solver has no
/// incentive to force-spend what's left, so unused money stays reported as
/// available instead of being padded arbitrarily into some envelope.
///
/// `min(a, b)` for absolute value is linearized with an auxiliary variable
/// `d_i` per envelope: `d_i >= x_i - target_i` and `d_i >= target_i - x_i`,
/// then minimizing `sum(priority_i * d_i)`.
#[uniffi::export]
pub fn suggest_allocation(
    available: f64,
    envelopes: Vec<EnvelopeInput>,
) -> Result<AllocationPlan, AllocationError> {
    if envelopes.is_empty() {
        return Err(AllocationError::NoEnvelopes);
    }
    if available < 0.0 {
        return Err(AllocationError::NegativeAvailable);
    }
    for envelope in &envelopes {
        if envelope.min_amount > envelope.max_amount {
            return Err(AllocationError::InvalidBounds {
                id: envelope.id.clone(),
            });
        }
    }

    // If the floors alone don't fit in what's available, no box-constrained
    // solve can ever be feasible. Scale every floor down proportionally
    // (each envelope keeps its relative share of the shortfall) rather than
    // failing outright — a degraded plan is more useful than none.
    let total_min: f64 = envelopes.iter().map(|e| e.min_amount).sum();
    let minimums_were_relaxed = total_min > available;
    let relaxation_factor = if minimums_were_relaxed && total_min > 0.0 {
        available / total_min
    } else {
        1.0
    };

    let mut vars = variables!();
    let allocations: Vec<_> = envelopes
        .iter()
        .map(|e| {
            let relaxed_min = e.min_amount * relaxation_factor;
            vars.add(
                variable()
                    .min(relaxed_min)
                    .max(e.max_amount.max(relaxed_min)),
            )
        })
        .collect();
    let deviations: Vec<_> = envelopes
        .iter()
        .map(|_| vars.add(variable().min(0.0)))
        .collect();

    let objective: Expression = deviations
        .iter()
        .zip(&envelopes)
        .map(|(d, e)| e.priority * *d)
        .sum();

    let mut model = vars.minimise(objective).using(good_lp::microlp);
    for ((allocation, deviation), envelope) in allocations.iter().zip(&deviations).zip(&envelopes) {
        model = model
            .with(constraint!(
                *deviation >= *allocation - envelope.historical_average
            ))
            .with(constraint!(
                *deviation >= envelope.historical_average - *allocation
            ));
    }
    let total: Expression = allocations.iter().map(|v| v.into_expression()).sum();
    model = model.with(constraint!(total <= available));

    let solution = model.solve().map_err(|_| AllocationError::Infeasible)?;

    let results = envelopes
        .iter()
        .zip(&allocations)
        .map(|(envelope, variable)| AllocationResult {
            id: envelope.id.clone(),
            suggested_amount: solution.value(*variable),
        })
        .collect();

    Ok(AllocationPlan {
        results,
        minimums_were_relaxed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(id: &str, average: f64, min: f64, max: f64, priority: f64) -> EnvelopeInput {
        EnvelopeInput {
            id: id.to_string(),
            historical_average: average,
            min_amount: min,
            max_amount: max,
            priority,
        }
    }

    #[test]
    fn matches_historical_average_when_budget_allows() {
        let plan = suggest_allocation(
            1000.0,
            vec![
                envelope("groceries", 400.0, 0.0, 600.0, 1.0),
                envelope("transport", 150.0, 0.0, 300.0, 1.0),
            ],
        )
        .unwrap();

        assert!(!plan.minimums_were_relaxed);
        let groceries = plan.results.iter().find(|r| r.id == "groceries").unwrap();
        let transport = plan.results.iter().find(|r| r.id == "transport").unwrap();
        assert!((groceries.suggested_amount - 400.0).abs() < 0.01);
        assert!((transport.suggested_amount - 150.0).abs() < 0.01);
    }

    #[test]
    fn respects_max_cap_even_with_room_to_spare() {
        let plan =
            suggest_allocation(1000.0, vec![envelope("rent", 900.0, 0.0, 800.0, 1.0)]).unwrap();
        let rent = &plan.results[0];
        assert!((rent.suggested_amount - 800.0).abs() < 0.01);
    }

    #[test]
    fn relaxes_minimums_proportionally_when_budget_is_short() {
        let plan = suggest_allocation(
            100.0,
            vec![
                envelope("a", 0.0, 80.0, 200.0, 1.0),
                envelope("b", 0.0, 40.0, 200.0, 1.0),
            ],
        )
        .unwrap();

        assert!(plan.minimums_were_relaxed);
        let total: f64 = plan.results.iter().map(|r| r.suggested_amount).sum();
        assert!(total <= 100.01);
    }

    #[test]
    fn rejects_inverted_bounds() {
        let err =
            suggest_allocation(100.0, vec![envelope("bad", 10.0, 50.0, 10.0, 1.0)]).unwrap_err();
        assert!(matches!(err, AllocationError::InvalidBounds { .. }));
    }

    #[test]
    fn rejects_empty_envelope_list() {
        let err = suggest_allocation(100.0, vec![]).unwrap_err();
        assert!(matches!(err, AllocationError::NoEnvelopes));
    }
}
