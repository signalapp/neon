use neon_runtime::raw::Env;
use neon_runtime::tsfn::{CallError, ThreadsafeFunction};

use crate::context::TaskContext;
use crate::result::NeonResult;

pub(crate) type Callback = Box<dyn FnOnce(Env) + Send + 'static>;

pub(crate) struct ThreadsafeTrampoline {
    tsfn: ThreadsafeFunction<Callback>,
}

impl ThreadsafeTrampoline {
    /// Creates an unbounded queue for scheduling closures on the JavaScript
    /// main thread
    pub(crate) fn new(env: Env) -> Self {
        let tsfn = unsafe { ThreadsafeFunction::new(env, Self::callback) };

        Self { tsfn: tsfn }
    }

    /// Schedules a closure to execute on the JavaScript thread that created
    /// this ThreadsafeTrampoline.
    /// Returns an `Error` if the task could not be scheduled.
    pub(crate) fn try_send<F>(&self, f: F) -> Result<(), CallError<Callback>>
    where
        F: FnOnce(TaskContext) -> NeonResult<()> + Send + 'static,
    {
        let callback = Box::new(move |env| {
            let env = unsafe { std::mem::transmute(env) };

            // Note: It is sufficient to use `TaskContext`'s `InheritedHandleScope` because
            // N-API creates a `HandleScope` before calling the callback.
            TaskContext::with_context(env, move |cx| {
                let _ = f(cx);
            });
        });

        self.tsfn.call(callback, None)
    }

    /// References a trampoline to prevent exiting the event loop until it has been dropped. (Default)
    /// Safety: `Env` must be valid for the current thread
    pub(crate) fn reference(&mut self, env: Env) {
        unsafe {
            self.tsfn.reference(env);
        }
    }

    /// Unreferences a trampoline to allow exiting the event loop before it has been dropped.
    /// Safety: `Env` must be valid for the current thread
    pub(crate) fn unref(&mut self, env: Env) {
        unsafe {
            self.tsfn.unref(env);
        }
    }

    // Monomorphized trampoline funciton for calling the user provided closure
    fn callback(env: Option<Env>, callback: Callback) {
        if let Some(env) = env {
            callback(env);
        } else {
            crate::context::internal::IS_RUNNING.with(|v| {
                *v.borrow_mut() = false;
            });
        }
    }
}
