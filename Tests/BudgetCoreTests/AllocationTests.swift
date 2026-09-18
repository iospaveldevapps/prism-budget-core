import Testing
@testable import BudgetCore

/// End-to-end tests through the UniFFI boundary: Swift structs are lowered
/// into the Rust solver and the plan is lifted back. These intentionally
/// mirror the Rust unit tests in `src/lib.rs` — if the two suites disagree,
/// the bindings (not the solver) are the suspect.
struct AllocationTests {
    private func envelope(
        _ id: String,
        average: Double,
        min: Double = 0,
        max: Double = 1_000_000,
        priority: Double = 1
    ) -> EnvelopeInput {
        EnvelopeInput(
            id: id,
            historicalAverage: average,
            minAmount: min,
            maxAmount: max,
            priority: priority
        )
    }

    @Test func matchesHistoricalAverageWhenBudgetAllows() throws {
        let plan = try suggestAllocation(
            available: 1000,
            envelopes: [
                envelope("groceries", average: 400, max: 600),
                envelope("transport", average: 150, max: 300)
            ]
        )

        #expect(!plan.minimumsWereRelaxed)
        let groceries = try #require(plan.results.first { $0.id == "groceries" })
        let transport = try #require(plan.results.first { $0.id == "transport" })
        #expect(abs(groceries.suggestedAmount - 400) < 0.01)
        #expect(abs(transport.suggestedAmount - 150) < 0.01)
    }

    @Test func respectsMaxCapEvenWithRoomToSpare() throws {
        let plan = try suggestAllocation(
            available: 1000,
            envelopes: [envelope("rent", average: 900, max: 800)]
        )
        let rent = try #require(plan.results.first)
        #expect(abs(rent.suggestedAmount - 800) < 0.01)
    }

    @Test func relaxesMinimumsProportionallyWhenBudgetIsShort() throws {
        let plan = try suggestAllocation(
            available: 100,
            envelopes: [
                envelope("a", average: 0, min: 80, max: 200),
                envelope("b", average: 0, min: 40, max: 200)
            ]
        )

        #expect(plan.minimumsWereRelaxed)
        let total = plan.results.reduce(0) { $0 + $1.suggestedAmount }
        #expect(total <= 100.01)
    }

    @Test func throwsOnInvertedBounds() {
        #expect(throws: AllocationError.self) {
            try suggestAllocation(
                available: 100,
                envelopes: [envelope("bad", average: 10, min: 50, max: 10)]
            )
        }
    }

    @Test func throwsOnEmptyEnvelopeList() {
        #expect(throws: AllocationError.self) {
            try suggestAllocation(available: 100, envelopes: [])
        }
    }

    @Test func errorsCarryLocalizedDescriptions() {
        do {
            _ = try suggestAllocation(available: -1, envelopes: [envelope("x", average: 1)])
            Issue.record("expected NegativeAvailable to be thrown")
        } catch {
            #expect(!error.localizedDescription.isEmpty)
        }
    }
}
