use std::{cell::Cell, future::Future, sync::Arc};

use elfo_utils::RateLimiter;

use crate::{
    addr::Addr,
    object::ObjectMeta,
    permissions::{AtomicPermissions, Permissions},
    trace_id::{self, TraceId},
};

tokio::task_local! {
    static SCOPE: Scope;
}

#[derive(Clone)]
pub struct Scope {
    actor: Addr,
    group: Addr,
    meta: Arc<ObjectMeta>,
    trace_id: Cell<TraceId>,

    // Per group.
    permissions: Arc<AtomicPermissions>,
    logging_limiter: Arc<RateLimiter>,
}

assert_impl_all!(Scope: Send);
assert_not_impl_all!(Scope: Sync);

impl Scope {
    /// Private API for now.
    #[doc(hidden)]
    pub fn new(
        actor: Addr,
        group: Addr,
        meta: Arc<ObjectMeta>,
        perm: Arc<AtomicPermissions>,
        logging_limiter: Arc<RateLimiter>,
    ) -> Self {
        Self::with_trace_id(
            trace_id::generate(),
            actor,
            group,
            meta,
            perm,
            logging_limiter,
        )
    }

    /// Private API for now.
    #[doc(hidden)]
    pub fn with_trace_id(
        trace_id: TraceId,
        actor: Addr,
        group: Addr,
        meta: Arc<ObjectMeta>,
        permissions: Arc<AtomicPermissions>,
        logging_limiter: Arc<RateLimiter>,
    ) -> Self {
        Self {
            actor,
            group,
            meta,
            trace_id: Cell::new(trace_id),
            permissions,
            logging_limiter,
        }
    }

    #[inline]
    #[deprecated(note = "use `actor()` instead")]
    pub fn addr(&self) -> Addr {
        self.actor
    }

    #[inline]
    pub fn actor(&self) -> Addr {
        self.actor
    }

    #[inline]
    pub fn group(&self) -> Addr {
        self.group
    }

    /// Returns the current object's meta.
    #[inline]
    pub fn meta(&self) -> &Arc<ObjectMeta> {
        &self.meta
    }

    /// Returns the current trace id.
    #[inline]
    pub fn trace_id(&self) -> TraceId {
        self.trace_id.get()
    }

    /// Replaces the current trace id with the provided one.
    #[inline]
    pub fn set_trace_id(&self, trace_id: TraceId) {
        self.trace_id.set(trace_id);
    }

    /// Returns the current permissions (for logging, telemetry and so on).
    #[inline]
    pub fn permissions(&self) -> Permissions {
        self.permissions.load()
    }

    /// Private API for now.
    #[inline]
    #[doc(hidden)]
    pub fn logging_limiter(&self) -> &RateLimiter {
        &self.logging_limiter
    }

    /// Wraps the provided future with the current scope.
    pub async fn within<F: Future>(self, f: F) -> F::Output {
        SCOPE.scope(self, f).await
    }

    /// Runs the provided function with the current scope.
    pub fn sync_within<R>(self, f: impl FnOnce() -> R) -> R {
        SCOPE.sync_scope(self, f)
    }
}

/// Exposes the current scope in order to send to other tasks.
///
/// # Panics
/// This function will panic if called outside actors.
pub fn expose() -> Scope {
    SCOPE.with(Clone::clone)
}

/// Exposes the current scope if inside the actor system.
pub fn try_expose() -> Option<Scope> {
    SCOPE.try_with(Clone::clone).ok()
}

/// Accesses the current scope and runs the provided closure.
///
/// # Panics
/// This function will panic if called ouside the actor system.
#[inline]
pub fn with<R>(f: impl FnOnce(&Scope) -> R) -> R {
    try_with(f).expect("cannot access a scope outside the actor system")
}

/// Accesses the current scope and runs the provided closure.
///
/// Returns `None` if called outside the actor system.
/// For a panicking variant, see `with`.
#[inline]
pub fn try_with<R>(f: impl FnOnce(&Scope) -> R) -> Option<R> {
    SCOPE.try_with(|scope| f(scope)).ok()
}

/// Returns the current trace id.
///
/// # Panics
/// This function will panic if called ouside the actor system.
#[inline]
pub fn trace_id() -> TraceId {
    with(Scope::trace_id)
}

/// Returns the current trace id if inside the actor system.
#[inline]
pub fn try_trace_id() -> Option<TraceId> {
    try_with(Scope::trace_id)
}

/// Replaces the current trace id with the provided one.
///
/// # Panics
/// This function will panic if called ouside the actor system.
#[inline]
pub fn set_trace_id(trace_id: TraceId) {
    with(|scope| scope.set_trace_id(trace_id));
}

/// Returns the current object's meta.
///
/// # Panics
/// This function will panic if called ouside the actor system.
#[inline]
pub fn meta() -> Arc<ObjectMeta> {
    with(|scope| scope.meta().clone())
}

/// Returns the current object's meta if inside the actor system.
#[inline]
pub fn try_meta() -> Option<Arc<ObjectMeta>> {
    try_with(|scope| scope.meta().clone())
}