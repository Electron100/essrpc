[![Latest Version](https://img.shields.io/crates/v/essrpc.svg)](https://crates.io/crates/essrpc)
[![docs](https://docs.rs/essrpc/badge.svg)](https://docs.rs/essrpc)

# Electron's Super Simple RPC (ESSRPC)
ESSRPC is a lightweight RPC library for Rust which aims to enable RPC
calls as transparently as possible through calls to ordinary trait
methods.

+  Allows ordinary calls to trait methods to call an implementation across an RPC boundary (in another process, across the network, etc)
+  Is agnostic to the 
+  Uses only stable Rust.

The magic is performed by the `essrpc` attribute macro which may
be applied to any trait whose functions each meet the following conditions:

+ Returns a `Result` whose error type implements `From<RPCError>`.
+ Uses only parameter and returns types which implement `Serialize`
+ Is not unsafe

Please [see the documentation](https://docs.rs/essrpc) for examples and more details.

# Status
Alpha. Things are expected to work, but little real world usage has occurred.

# Configurable Features
+ `async_client`: Enables [asynchronous clients](https://docs.rs/essrpc/0.2.0/essrpc/#asynchronous-clients)
+ `bincode_transport`: Enables [BincodeTransport](https://docs.rs/essrpc/0.1.1/essrpc/transports/struct.BincodeTransport.html)
+ `json_transport`: Enables [JSONTransport](https://docs.rs/essrpc/0.1.1/essrpc/transports/struct.JSONTransport.html)
+ `wasm_bindgen`: Enables wasm-bindgen compatibility. Specifically,
  UUID generation in the JSON transport uses wasm-bindgen compatible
  randomness.

# Inspirations and Motivations
ESSRPC was inspired by **[tarpc](https://github.com/google/tarpc)** and by the `build_rpc_trait!` macro
from **[jsonrpc](https://github.com/paritytech/jsonrpc)**. Both of these are more mature projects. The recent
stabilization of procedural macros allows ESSRPC to generate an RPC
client/server pair from a more natural trait defintion. ESSRPC also makes
fewer assumptions about the underlying RPC transport.

