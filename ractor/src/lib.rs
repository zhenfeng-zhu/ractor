// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

//! `ractor`: A pure-Rust actor framework. Inspired from [Erlang's `gen_server`](https://www.erlang.org/doc/man/gen_server.html),
//! with the speed + performance of Rust!
//!
//! ## Installation
//!
//! Install `ractor` by adding the following to your Cargo.toml dependencies
//!
//! ```toml
//! [dependencies]
//! ractor = "0.1"
//! ```
//!
//! ## Getting started
//!
//! An example "ping-pong" actor might be the following
//!
//! ```rust
//! use ractor::{Actor, ActorRef};
//!
//! /// [PingPong] is a basic actor that will print
//! /// ping..pong.. repeatedly until some exit
//! /// condition is met (a counter hits 10). Then
//! /// it will exit
//! pub struct PingPong;
//!
//! /// This is the types of message [PingPong] supports
//! #[derive(Debug, Clone)]
//! pub enum Message {
//!     Ping,
//!     Pong,
//! }
//!
//! impl Message {
//!     // retrieve the next message in the sequence
//!     fn next(&self) -> Self {
//!         match self {
//!             Self::Ping => Self::Pong,
//!             Self::Pong => Self::Ping,
//!         }
//!     }
//!     // print out this message
//!     fn print(&self) {
//!         match self {
//!             Self::Ping => print!("ping.."),
//!             Self::Pong => print!("pong.."),
//!         }
//!     }
//! }
//!
//! // the implementation of our actor's "logic"
//! #[async_trait::async_trait]
//! impl Actor for PingPong {
//!     // An actor has a message type
//!     type Msg = Message;
//!     // and (optionally) internal state
//!     type State = u8;
//!
//!     // Initially we need to create our state, and potentially
//!     // start some internal processing (by posting a message for
//!     // example)
//!     async fn pre_start(&self, myself: ActorRef<Self>) -> Self::State {
//!         // startup the event processing
//!         myself.send_message(Message::Ping).unwrap();
//!         0u8
//!     }
//!
//!     // This is our main message handler
//!     async fn handle(
//!         &self,
//!         myself: ActorRef<Self>,
//!         message: Self::Msg,
//!         state: &mut Self::State,
//!     ) {
//!         if *state < 10u8 {
//!             message.print();
//!             myself.send_message(message.next()).unwrap();
//!             *state += 1;
//!         } else {
//!             myself.stop(None);
//!             // don't send another message, rather stop the agent after 10 iterations
//!         }
//!     }
//! }
//!
//! async fn run() {
//!     let (_, actor_handle) = Actor::spawn(None, PingPong).await.expect("Failed to start actor");
//!     actor_handle.await.expect("Actor failed to exit cleanly");
//! }
//! ```
//!
//! which will output
//!
//! ```bash
//! $ cargo run
//! ping..pong..ping..pong..ping..pong..ping..pong..ping..pong..
//! $
//! ```
//!
//! ## Supervision
//!
//! Actors in `ractor` also support supervision. This is done by "linking" actors together in a supervisor-child relationship.
//! A supervisor is "responsible" for the child actor, and as such is notified when the actor starts, stops, and fails (panics).
//!
//! Supervision is presently left to the implementor, but you can see a suite of supervision tests in `crate::actor::tests::supervisor`
//! for examples on the supported functionality.
//!
//! NOTE: panic's in `pre_start` of an actor will cause failures to spawn, rather than supervision notified failurs as the actor hasn't "linked"
//! to its supervisor yet. However failures in `post_start`, `handle`, `handle_supervisor_evt`, `post_stop` will notify the supervisor should a failure
//! occur. See [crate::Actor] documentation for more information
//!
//! ## Messaging actors
//!
//! The means of communication between actors is that they pass messages to each other. A developer can define any message type which is `Send + 'static` and it
//! will be supported by `ractor`. There are 4 concurrent message types, which are listened to in priority. They are
//!
//! 1. Signals: Signals are the highest-priority of all and will interrupt the actor wherever processing currently is (this includes terminating async work). There
//! is only 1 signal today, which is `Signal::Kill`, and it immediately terminates all work. This includes message processing or supervision event processing.
//! 2. Stop: There is also a pre-defined stop signal. You can give a "stop reason" if you want, but it's optional. Stop is a graceful exit, meaning currently executing async
//! work will complete, and on the next message processing iteration Stop will take prioritity over future supervision events or regular messages. It will **not** terminate
//! currently executing work, regardless of the provided reason.
//! 3. SupervisionEvent: Supervision events are messages from child actors to their supervisors in the event of their startup, death, and/or unhandled panic. Supervision events
//! are how an actor's supervisor(s) are notified of events of their children and can handle lifetime events for them.
//! 4. Messages: Regular, user-defined, messages are the last channel of communication to actors. They are the lowest priority of the 4 message types and denote general actor work. The first
//! 3 messages types (signals, stop, supervision) are generally quiet unless it's a lifecycle event for the actor, but this channel is the "work" channel doing what your actor wants to do!

#![deny(warnings)]
#![warn(unused_imports)]
#![warn(unsafe_code)]
#![warn(missing_docs)]
#![warn(unused_crate_dependencies)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use std::any::Any;

/// An actor's name, equivalent to an [Erlang `atom()`](https://www.erlang.org/doc/reference_manual/data_types.html#atom)
pub type ActorName = &'static str;

/// A process group's name, equivalent to an [Erlang `atom()`](https://www.erlang.org/doc/reference_manual/data_types.html#atom)
pub type GroupName = &'static str;

pub mod actor;
pub mod actor_id;
pub mod pg;
pub mod port;
pub mod registry;
pub mod rpc;
pub mod time;

#[cfg(test)]
use criterion as _;
#[cfg(test)]
use rand as _;

// WIP
// #[cfg(feature = "remote")]
// pub mod distributed;

// re-exports
pub use actor::actor_cell::{ActorCell, ActorRef, ActorStatus, ACTIVE_STATES};
pub use actor::errors::{ActorErr, MessagingErr, SpawnErr};
pub use actor::messages::{Signal, SupervisionEvent};
pub use actor::{Actor, ActorRuntime};
pub use actor_id::ActorId;
pub use port::{OutputMessage, OutputPort, RpcReplyPort};

/// Message type for an actor. Generally an enum
/// which muxes the various types of inner-messages the actor
/// supports
///
/// ## Example
///
/// ```rust
/// pub enum MyMessage {
///     /// Record the name to the actor state
///     RecordName(String),
///     /// Print the recorded name from the state to command line
///     PrintName,
/// }
/// ```
pub trait Message: Any + Send + 'static {}
impl<T: Any + Send + 'static> Message for T {}

/// Represents the state of an actor. Must be safe
/// to send between threads (same bounds as a [Message])
pub trait State: Message {}
impl<T: Message> State for T {}
