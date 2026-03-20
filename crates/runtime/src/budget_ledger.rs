//! Session Budget Ledger
//!
//! Thread-safe budget tracking with reserve/settle semantics for provider calls.
//! Supports parent/child ledger hierarchy where child ledgers deduct from parent.
//!
//! # Usage Pattern
//!
//! 1. Before provider call: `ledger.reserve(estimated_cost)?`
//! 2. After provider call: `reservation.settle(actual_cost)`
//! 3. Check remaining: `ledger.remaining()`
//!
//! The reserve/settle pattern allows for bounded overrun semantics where a turn
//! that exceeds its estimate is allowed to complete, but future turns are blocked.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Error returned when budget is exhausted and cannot accommodate a reservation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetExhausted {
    /// The amount of budget that was requested.
    pub requested: u64,
    /// The amount of budget remaining at the time of the request.
    pub remaining: u64,
}

impl std::fmt::Display for BudgetExhausted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Budget exhausted: requested {} micro-USD but only {} remaining",
            self.requested, self.remaining
        )
    }
}

impl std::error::Error for BudgetExhausted {}

/// A snapshot of the current budget state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetSnapshot {
    /// Total budget that was initially allocated.
    pub initial: u64,
    /// Budget currently reserved but not yet settled.
    pub reserved: u64,
    /// Budget that has been settled (actual spend).
    pub settled: u64,
    /// Budget remaining (initial - reserved - settled).
    pub remaining: u64,
}

impl BudgetSnapshot {
    /// Returns true if the budget is completely exhausted.
    pub fn is_exhausted(&self) -> bool {
        self.remaining == 0
    }

    /// Returns the amount of budget that has been spent (settled).
    pub fn spent(&self) -> u64 {
        self.settled
    }

    /// Returns the amount of budget pending settlement (reserved - settled).
    pub fn pending(&self) -> u64 {
        self.reserved.saturating_sub(self.settled)
    }
}

/// A reservation of budget for a pending operation.
///
/// When dropped without calling `settle()`, the reserved amount is returned
/// to the available budget (cancelled reservation).
#[derive(Debug)]
pub struct Reservation {
    ledger: Arc<BudgetLedgerInner>,
    reserved_amount: u64,
    settled: bool,
}

impl Reservation {
    /// Settle the reservation with the actual cost.
    ///
    /// The actual cost may be higher or lower than the reserved amount.
    /// If higher, the difference is deducted from the ledger.
    /// If lower, the difference is returned to the ledger.
    ///
    /// # Arguments
    ///
    /// * `actual_cost` - The actual cost in micro-USD
    ///
    /// # Returns
    ///
    /// The difference between reserved and actual (positive means over-reserved).
    pub fn settle(mut self, actual_cost: u64) -> i64 {
        self.settled = true;
        self.ledger.settle(self.reserved_amount, actual_cost)
    }

    /// Cancel the reservation without settling.
    ///
    /// This returns the reserved amount to the available budget.
    pub fn cancel(mut self) {
        self.settled = true;
        self.ledger.cancel_reservation(self.reserved_amount);
    }

    /// Get the amount that was reserved.
    pub fn reserved_amount(&self) -> u64 {
        self.reserved_amount
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if !self.settled {
            // Reservation was dropped without settling - cancel it
            self.ledger.cancel_reservation(self.reserved_amount);
        }
    }
}

/// Inner shared state for budget tracking.
#[derive(Debug)]
struct BudgetLedgerInner {
    /// Initial budget amount in micro-USD.
    initial: u64,
    /// Currently reserved amount (atomic for thread safety).
    reserved: AtomicU64,
    /// Settled/spent amount (atomic for thread safety).
    settled: AtomicU64,
    /// Optional parent ledger to report settlements to.
    parent: Option<Arc<BudgetLedgerInner>>,
}

impl BudgetLedgerInner {
    fn new(initial: u64, parent: Option<Arc<BudgetLedgerInner>>) -> Self {
        Self {
            initial,
            reserved: AtomicU64::new(0),
            settled: AtomicU64::new(0),
            parent,
        }
    }

    /// Attempt to reserve budget using a CAS loop to avoid TOCTOU races.
    /// Returns true if successful.
    fn try_reserve(&self, amount: u64) -> bool {
        loop {
            let reserved = self.reserved.load(Ordering::SeqCst);
            let settled = self.settled.load(Ordering::SeqCst);
            let remaining = self
                .initial
                .saturating_sub(reserved)
                .saturating_sub(settled);

            if amount > remaining {
                return false;
            }

            match self.reserved.compare_exchange(
                reserved,
                reserved + amount,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return true,
                Err(_) => continue,
            }
        }
    }

    /// Settle a reservation with actual cost.
    fn settle(&self, reserved_amount: u64, actual_cost: u64) -> i64 {
        self.settled.fetch_add(actual_cost, Ordering::SeqCst);
        let diff = reserved_amount as i64 - actual_cost as i64;
        self.reserved.fetch_sub(reserved_amount, Ordering::SeqCst);
        diff
    }

    /// Cancel a reservation, returning it to available budget.
    fn cancel_reservation(&self, amount: u64) {
        self.reserved.fetch_sub(amount, Ordering::SeqCst);
    }

    /// Get current remaining budget.
    fn remaining(&self) -> u64 {
        let reserved = self.reserved.load(Ordering::SeqCst);
        let settled = self.settled.load(Ordering::SeqCst);
        self.initial
            .saturating_sub(reserved)
            .saturating_sub(settled)
    }

    /// Get current snapshot.
    fn snapshot(&self) -> BudgetSnapshot {
        let reserved = self.reserved.load(Ordering::SeqCst);
        let settled = self.settled.load(Ordering::SeqCst);
        let remaining = self
            .initial
            .saturating_sub(reserved)
            .saturating_sub(settled);

        BudgetSnapshot {
            initial: self.initial,
            reserved,
            settled,
            remaining,
        }
    }
}

/// Thread-safe budget ledger with reserve/settle semantics.
///
/// The ledger tracks budget using atomic operations for thread safety.
/// It supports a parent/child hierarchy where child ledgers deduct from parent.
///
/// # Example
///
/// ```
/// use runtime::BudgetLedger;
///
/// // Create a ledger with 1000 micro-USD budget
/// let ledger = BudgetLedger::new(1000);
///
/// // Reserve budget before a provider call
/// let reservation = ledger.reserve(100).expect("budget available");
///
/// // ... make provider call ...
/// let actual_cost = 80;
///
/// // Settle with actual cost
/// let diff = reservation.settle(actual_cost);
/// assert_eq!(diff, 20); // 20 micro-USD returned to ledger
///
/// // Check remaining budget
/// let snapshot = ledger.remaining();
/// assert_eq!(snapshot.remaining, 920);
/// ```
#[derive(Debug, Clone)]
pub struct BudgetLedger {
    inner: Arc<BudgetLedgerInner>,
}

impl BudgetLedger {
    /// Create a new root ledger with the given initial budget.
    ///
    /// # Arguments
    ///
    /// * `initial` - The initial budget in micro-USD
    pub fn new(initial: u64) -> Self {
        Self {
            inner: Arc::new(BudgetLedgerInner::new(initial, None)),
        }
    }

    /// Create a child ledger with an allocated budget from this parent.
    ///
    /// The child ledger receives its own budget pool deducted from the parent.
    /// When the child settles costs, they are reported to the parent for
    /// tracking purposes. This allows for hierarchical budget management.
    ///
    /// # Arguments
    ///
    /// * `allocation` - The budget allocated to the child in micro-USD
    ///
    /// # Returns
    ///
    /// A new child ledger, or `None` if the allocation exceeds remaining budget.
    pub fn create_child(&self, allocation: u64) -> Option<BudgetLedger> {
        // Check if parent has enough remaining budget
        if allocation > self.inner.remaining() {
            return None;
        }

        // Deduct allocation from parent's remaining budget by adding to settled
        // This represents budget "allocated to child" that can't be used elsewhere
        self.inner.settled.fetch_add(allocation, Ordering::SeqCst);

        // Create child ledger with this as parent for settlement reporting
        let child_inner = Arc::new(BudgetLedgerInner::new(allocation, Some(self.inner.clone())));

        Some(BudgetLedger { inner: child_inner })
    }

    /// Return any unused child allocation back to the parent ledger.
    /// Call this when a child ledger is done to avoid stranding budget.
    pub fn release_unused(&self) {
        if let Some(ref parent) = self.inner.parent {
            let settled = self.inner.settled.load(Ordering::SeqCst);
            let unused = self.inner.initial.saturating_sub(settled);
            if unused > 0 {
                parent.settled.fetch_sub(unused, Ordering::SeqCst);
            }
        }
    }

    /// Reserve budget for an upcoming operation.
    ///
    /// Returns a `Reservation` that must be settled with the actual cost.
    /// If the budget is exhausted, returns `BudgetExhausted`.
    ///
    /// # Arguments
    ///
    /// * `estimated_cost` - The estimated cost to reserve in micro-USD
    ///
    /// # Returns
    ///
    /// `Ok(Reservation)` on success, `Err(BudgetExhausted)` if insufficient budget.
    pub fn reserve(&self, estimated_cost: u64) -> Result<Reservation, BudgetExhausted> {
        if estimated_cost == 0 {
            // Zero-cost reservation always succeeds
            return Ok(Reservation {
                ledger: self.inner.clone(),
                reserved_amount: 0,
                settled: false,
            });
        }

        if self.inner.try_reserve(estimated_cost) {
            Ok(Reservation {
                ledger: self.inner.clone(),
                reserved_amount: estimated_cost,
                settled: false,
            })
        } else {
            let remaining = self.inner.remaining();
            Err(BudgetExhausted {
                requested: estimated_cost,
                remaining,
            })
        }
    }

    /// Get a snapshot of the current budget state.
    ///
    /// This is useful for monitoring and reporting budget usage.
    pub fn remaining(&self) -> BudgetSnapshot {
        self.inner.snapshot()
    }

    /// Get the initial budget amount.
    pub fn initial_budget(&self) -> u64 {
        self.inner.initial
    }

    /// Check if the budget is completely exhausted.
    pub fn is_exhausted(&self) -> bool {
        self.inner.remaining() == 0
    }

    /// Force-settle a cost without a prior reservation.
    ///
    /// This is useful for retroactive cost accounting when a reservation
    /// wasn't made beforehand. Returns the amount actually deducted (may
    /// be less than requested if budget is exhausted).
    ///
    /// # Arguments
    ///
    /// * `cost` - The cost to settle in micro-USD
    ///
    /// # Returns
    ///
    /// The amount actually deducted from the budget.
    pub fn force_settle(&self, cost: u64) -> u64 {
        let remaining = self.inner.remaining();
        let actual = cost.min(remaining);
        self.inner.settled.fetch_add(actual, Ordering::SeqCst);

        // Also deduct from parent if present
        if let Some(ref parent) = self.inner.parent {
            parent.settled.fetch_add(actual, Ordering::SeqCst);
        }

        actual
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_budget_ledger_creation() {
        let ledger = BudgetLedger::new(1000);
        let snapshot = ledger.remaining();

        assert_eq!(snapshot.initial, 1000);
        assert_eq!(snapshot.reserved, 0);
        assert_eq!(snapshot.settled, 0);
        assert_eq!(snapshot.remaining, 1000);
        assert!(!ledger.is_exhausted());
    }

    #[test]
    fn test_reserve_and_settle_exact() {
        let ledger = BudgetLedger::new(1000);

        let reservation = ledger.reserve(100).expect("reserve should succeed");
        assert_eq!(reservation.reserved_amount(), 100);

        let diff = reservation.settle(100);
        assert_eq!(diff, 0);

        let snapshot = ledger.remaining();
        assert_eq!(snapshot.settled, 100);
        assert_eq!(snapshot.remaining, 900);
    }

    #[test]
    fn test_reserve_and_settle_under() {
        let ledger = BudgetLedger::new(1000);

        let reservation = ledger.reserve(100).expect("reserve should succeed");
        let diff = reservation.settle(80);

        assert_eq!(diff, 20); // 20 returned to ledger
        let snapshot = ledger.remaining();
        assert_eq!(snapshot.settled, 80);
        assert_eq!(snapshot.remaining, 920);
    }

    #[test]
    fn test_reserve_and_settle_over() {
        let ledger = BudgetLedger::new(1000);

        let reservation = ledger.reserve(100).expect("reserve should succeed");
        let diff = reservation.settle(120);

        assert_eq!(diff, -20); // 20 extra deducted
        let snapshot = ledger.remaining();
        assert_eq!(snapshot.settled, 120);
        assert_eq!(snapshot.remaining, 880);
    }

    #[test]
    fn test_reserve_exhausted_budget() {
        let ledger = BudgetLedger::new(100);

        // First reservation succeeds
        let r1 = ledger.reserve(100).expect("first reserve should succeed");
        r1.settle(100);

        // Second reservation fails - budget exhausted
        let err = ledger
            .reserve(1)
            .expect_err("should fail with exhausted budget");
        assert_eq!(err.requested, 1);
        assert_eq!(err.remaining, 0);
    }

    #[test]
    fn test_cancel_reservation() {
        let ledger = BudgetLedger::new(1000);

        let reservation = ledger.reserve(100).expect("reserve should succeed");
        let snapshot_before = ledger.remaining();
        assert_eq!(snapshot_before.reserved, 100);

        reservation.cancel();

        let snapshot_after = ledger.remaining();
        assert_eq!(snapshot_after.reserved, 0);
        assert_eq!(snapshot_after.remaining, 1000);
    }

    #[test]
    fn test_reservation_drop_cancels() {
        let ledger = BudgetLedger::new(1000);

        {
            let _reservation = ledger.reserve(100).expect("reserve should succeed");
            let snapshot = ledger.remaining();
            assert_eq!(snapshot.reserved, 100);
            // Reservation dropped here
        }

        let snapshot = ledger.remaining();
        assert_eq!(snapshot.reserved, 0);
        assert_eq!(snapshot.remaining, 1000);
    }

    #[test]
    fn test_zero_cost_reservation() {
        let ledger = BudgetLedger::new(1000);

        let reservation = ledger.reserve(0).expect("zero reserve should succeed");
        assert_eq!(reservation.reserved_amount(), 0);

        let diff = reservation.settle(0);
        assert_eq!(diff, 0);

        let snapshot = ledger.remaining();
        assert_eq!(snapshot.remaining, 1000);
    }

    #[test]
    fn test_child_ledger_creation() {
        let parent = BudgetLedger::new(1000);
        let child = parent
            .create_child(300)
            .expect("child creation should succeed");

        // Parent should show the allocation as settled (transferred to child)
        let parent_snapshot = parent.remaining();
        assert_eq!(parent_snapshot.settled, 300);
        assert_eq!(parent_snapshot.remaining, 700);

        // Child should have its own budget
        let child_snapshot = child.remaining();
        assert_eq!(child_snapshot.initial, 300);
        assert_eq!(child_snapshot.remaining, 300);
    }

    #[test]
    fn test_child_ledger_reserve_deducts_from_parent() {
        let parent = BudgetLedger::new(1000);
        let child = parent
            .create_child(300)
            .expect("child creation should succeed");

        // Reserve from child — child is self-contained, does NOT propagate to parent
        let reservation = child.reserve(100).expect("reserve should succeed");

        // Parent sees only the pre-allocation (300 settled), not child's reservation
        let parent_snapshot = parent.remaining();
        assert_eq!(parent_snapshot.reserved, 0);
        assert_eq!(parent_snapshot.settled, 300);

        // Child tracks its own reservation
        let child_snapshot = child.remaining();
        assert_eq!(child_snapshot.reserved, 100);

        // Settle
        reservation.settle(80);

        // Parent still sees only the pre-allocation — no double-counting
        let parent_snapshot = parent.remaining();
        assert_eq!(parent_snapshot.reserved, 0);
        assert_eq!(parent_snapshot.settled, 300); // only allocation, not allocation + actual

        let child_snapshot = child.remaining();
        assert_eq!(child_snapshot.settled, 80);
        assert_eq!(child_snapshot.remaining, 220);
    }

    #[test]
    fn test_child_settlement_no_double_count() {
        let parent = BudgetLedger::new(1000);
        let child = parent
            .create_child(500)
            .expect("child creation should succeed");

        // Child spends 300
        let r = child.reserve(300).expect("reserve should succeed");
        r.settle(300);

        // Parent should see 300 spent (not 800 = 500 allocation + 300 actual)
        let parent_snapshot = parent.remaining();
        assert_eq!(parent_snapshot.settled, 500); // only the pre-allocation

        // Release unused child budget back to parent
        child.release_unused();
        let parent_snapshot = parent.remaining();
        // 500 pre-allocated - 200 unused returned = 300 net settled
        assert_eq!(parent_snapshot.settled, 300);
        assert_eq!(parent_snapshot.remaining, 700);
    }

    #[test]
    fn test_child_ledger_exceeds_parent() {
        let parent = BudgetLedger::new(100);

        // Cannot create child with more than parent has
        let child = parent.create_child(200);
        assert!(child.is_none());
    }

    #[test]
    fn test_child_reservation_exceeds_parent_remaining() {
        let parent = BudgetLedger::new(1000);
        let child = parent
            .create_child(500)
            .expect("child creation should succeed");

        // Use up parent's remaining budget (500 left after child allocation)
        let parent_reservation = parent.reserve(500).expect("reserve should succeed");
        parent_reservation.settle(500);

        // Child should still be able to use its allocated budget
        let child_reservation = child.reserve(300).expect("child reserve should succeed");
        child_reservation.settle(300);

        // But child cannot exceed its allocation
        let err = child.reserve(300).expect_err("should fail");
        assert_eq!(err.requested, 300);
        assert_eq!(err.remaining, 200); // 500 - 300 = 200
    }

    #[test]
    fn test_force_settle() {
        let ledger = BudgetLedger::new(100);

        let deducted = ledger.force_settle(50);
        assert_eq!(deducted, 50);

        let snapshot = ledger.remaining();
        assert_eq!(snapshot.settled, 50);
        assert_eq!(snapshot.remaining, 50);

        // Force settle more than remaining
        let deducted = ledger.force_settle(100);
        assert_eq!(deducted, 50); // Only 50 remaining

        let snapshot = ledger.remaining();
        assert_eq!(snapshot.settled, 100);
        assert_eq!(snapshot.remaining, 0);
    }

    #[test]
    fn test_budget_exhausted_display() {
        let err = BudgetExhausted {
            requested: 100,
            remaining: 50,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("100"));
        assert!(msg.contains("50"));
        assert!(msg.contains("micro-USD"));
    }

    #[test]
    fn test_budget_snapshot_helpers() {
        let snapshot = BudgetSnapshot {
            initial: 1000,
            reserved: 100,
            settled: 200,
            remaining: 700,
        };

        assert!(!snapshot.is_exhausted());
        assert_eq!(snapshot.spent(), 200);
        assert_eq!(snapshot.pending(), 0); // reserved - settled = 0 in this case

        let snapshot2 = BudgetSnapshot {
            initial: 1000,
            reserved: 300,
            settled: 200,
            remaining: 500,
        };
        assert_eq!(snapshot2.pending(), 100); // 300 - 200 = 100

        let snapshot3 = BudgetSnapshot {
            initial: 100,
            reserved: 50,
            settled: 50,
            remaining: 0,
        };
        assert!(snapshot3.is_exhausted());
    }

    #[test]
    fn test_concurrent_reservations() {
        use std::thread;

        let ledger = BudgetLedger::new(1000);
        let ledger_arc = Arc::new(ledger);

        let mut handles = vec![];

        // Spawn 10 threads, each reserving 100
        for _ in 0..10 {
            let ledger_clone = Arc::clone(&ledger_arc);
            let handle = thread::spawn(move || ledger_clone.reserve(100));
            handles.push(handle);
        }

        // Collect results
        let mut success_count = 0;
        let mut fail_count = 0;
        for handle in handles {
            match handle.join().unwrap() {
                Ok(reservation) => {
                    success_count += 1;
                    // Settle with actual cost
                    reservation.settle(100);
                }
                Err(_) => fail_count += 1,
            }
        }

        // All 10 should succeed (10 * 100 = 1000)
        assert_eq!(success_count, 10);
        assert_eq!(fail_count, 0);

        let snapshot = ledger_arc.remaining();
        assert_eq!(snapshot.settled, 1000);
        assert_eq!(snapshot.remaining, 0);
    }

    #[test]
    fn test_concurrent_over_reservation() {
        use std::thread;

        let ledger = BudgetLedger::new(500);
        let ledger_arc = Arc::new(ledger);

        let mut handles = vec![];

        // Spawn 10 threads, each reserving 100 (total 1000 > 500)
        for _ in 0..10 {
            let ledger_clone = Arc::clone(&ledger_arc);
            let handle = thread::spawn(move || ledger_clone.reserve(100));
            handles.push(handle);
        }

        // Collect results
        let mut success_count = 0;
        let mut fail_count = 0;
        for handle in handles {
            match handle.join().unwrap() {
                Ok(reservation) => {
                    success_count += 1;
                    reservation.settle(100);
                }
                Err(_) => fail_count += 1,
            }
        }

        // Only 5 should succeed (5 * 100 = 500)
        assert_eq!(success_count, 5);
        assert_eq!(fail_count, 5);

        let snapshot = ledger_arc.remaining();
        assert_eq!(snapshot.settled, 500);
        assert_eq!(snapshot.remaining, 0);
    }

    #[test]
    fn test_concurrent_reservation_safety() {
        use std::thread;

        let budget = 1000u64;
        let amount_per_thread = 10u64;
        let num_threads = 200usize;
        let ledger = Arc::new(BudgetLedger::new(budget));

        let handles: Vec<_> = (0..num_threads)
            .map(|_| {
                let l = Arc::clone(&ledger);
                thread::spawn(move || l.reserve(amount_per_thread))
            })
            .collect();

        let mut success_count = 0u64;
        for handle in handles {
            if let Ok(reservation) = handle.join().unwrap() {
                success_count += 1;
                reservation.settle(amount_per_thread);
            }
        }

        assert!(
            success_count * amount_per_thread <= budget,
            "total reserved ({}) exceeded budget ({})",
            success_count * amount_per_thread,
            budget
        );
        assert_eq!(success_count, budget / amount_per_thread);
    }

    #[test]
    fn test_deep_parent_child_chain() {
        let grandparent = BudgetLedger::new(1000);
        let parent = grandparent
            .create_child(600)
            .expect("parent creation should succeed");
        let child = parent
            .create_child(300)
            .expect("child creation should succeed");

        // Reserve from child — child is self-contained, no propagation
        let reservation = child.reserve(100).expect("reserve should succeed");
        assert_eq!(child.remaining().reserved, 100);
        assert_eq!(parent.remaining().reserved, 0);
        assert_eq!(grandparent.remaining().reserved, 0);

        reservation.settle(80);

        // Each ledger sees only its own pre-allocation, not child's actual spend
        assert_eq!(grandparent.remaining().settled, 600); // only grandparent->parent allocation
        assert_eq!(parent.remaining().settled, 300); // only parent->child allocation
        assert_eq!(child.remaining().settled, 80);
    }

    #[test]
    fn test_child_ledger_clone_independence() {
        let parent = BudgetLedger::new(1000);
        let child1 = parent
            .create_child(400)
            .expect("child1 creation should succeed");
        let child2 = parent
            .create_child(400)
            .expect("child2 creation should succeed");

        let r1 = child1.reserve(300).expect("reserve should succeed");
        r1.settle(300);

        assert_eq!(child2.remaining().remaining, 400);

        // Parent sees only the two allocations (800), not child1's actual spend
        let parent_snapshot = parent.remaining();
        assert_eq!(parent_snapshot.settled, 800);
        assert_eq!(parent_snapshot.remaining, 200);
    }
}
