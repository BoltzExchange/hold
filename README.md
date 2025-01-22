# hold

A CLN hold invoice plugin

## Installation

### Compilation

Requirements:

- Rust (latest stable version recommended)
- `gcc`
- `libpq-dev`
- `libsqlite3-dev`

To compile `hold` run:

```bash
cargo build --release
```

## Usage

### Options

#### Database

`hold-database` sets the database that should be used

This can be:

- SQLite: `sqlite://<path>`
- PostgreSQL: `postgresql://<username>:<password>@<host>:<port>/<database>`

#### gRPC

`hold-grpc-host` the host on which the gRPC server should listen to

`hold-grpc-port` the port on which the gRPC server should listen to

#### Advanced

`hold-mpp-timeout` the MPP timeout of payment shards in seconds.
Default is 60.
_Should only be changed for debugging and testing purposes_

### Commands

- `holdinvoice payment_hash amount`: creates a new hold invoice
- `injectinvoice invoice`: injects a new invoice into the hold plugin
- `listholdinvoices [payment_hash] [invoice]`: lists existing hold invoices
- `settleholdinvoice preimage`: settles a hold invoice
- `cancelholdinvoice payment_hash`: cancels a hold invoice

More hold invoice creation parameters and streaming calls for updates are available in the gRPC interface.

### gRPC

`hold` also exposes a gRPC interface.
The server is serving with TLS,
and the client needs to authenticate itself with the `client` certificates and the plugin creates.
Similarly to how the gRPC plugin itself does it

The protobuf definitions can be found [here](https://github.com/BoltzExchange/hold/blob/main/protos/hold.proto)
