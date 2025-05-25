[![Review Assignment Due Date](https://classroom.github.com/assets/deadline-readme-button-22041afd0340ce965d47ae6ef1cefeee28c7c493a6346c4f15d667ab976d596c.svg)](https://classroom.github.com/a/YmMajyCa)

Para información sobre setup del proyecto ver el documento [`./docs/contribute.md`](./docs/contribute.md).

---

# PedidosRust - Grupo MutexMasters

<p align="center">
    <img src="./docs/imgs/pedidos_rust_logo.png" alt="PedidosRust logo" height="300px">
</p>

PedidosRust es una nueva aplicación para conectar restaurantes, repartidores y comensales. Gracias a su innovadora implementación distribuida, permitirá reducir los costos y apuntar a ser líder en el mercado.

Los comensales podrán solicitar un pedido a un restaurante, los restaurantes notifican cuando el pedido está listo, y los repartidores buscan pedidos cercanos y los entregan.

### Integrantes

| Nombre               | Padrón | Email           |
| -------------------- | ------ | --------------- |
| Avalos, Victoria     | 108434 | -               |
| Chacón, Ignacio      |        | -               |
| Diem, Walter Gabriel | 105618 | wdiem@fi.uba.ar |
| Funes, Nicolas       |        | -               |

---

### Table of contents

1. [Aplicaciones](#Aplicaciones)
1. [Estructura del repositorio y sistema](#Estructura-del-repositorio-y-sistema)
1. [Uso](#Uso)
1. [Diseño](#Diseño)
    1. [Pedidos-rust](#Pedidos-rust)
    1. [Customer](#Customer)
    1. [Rider](#Rider)
    1. [Restaurant](#Restaurant)
    1. [Payment](#Payment)
1. [Mensajes](#Mensajes)

## Aplicaciones

Hay 5 aplicaciones ejecutables, las cuales son referenciados por sus nombres en inglés (por consistencia con la convención de programación):

- Requeridas por la consigna (4 apps): comensales (`customers`), restaurantes (`restaurants`), repartidores (`riders`), gateway de pagos (`payment`).

- Adicional (1 app): aplicación para simplificar el manejo de mensajes entre actores llamada `pedidos-rust`.

## Estructura del repositorio y sistema

El root del repositorio es un workspace que contiene los proyectos (ejecutables binarios separados, cada uno con su `Cargo.toml`) y el código en común:

```
.
├── common          Código compartido entre aplicaciones y protocolo
├── customer        Project del customer
├── docs
├── payment         Project del payment gateway
├── pedidos-rust    Project del pedidos-rust
├── restaurant      Project del restaurant
├── rider           Project del rider
└── Cargo.toml      Configuración del workspace
```

## Uso

Se deja las instrucciones para ejecutar las diversas apps:

**Customer:**

```bash
cargo run -p customer
```

**Payment:**

```bash
cargo run -p payment
```

**Pedidos-rust:**

```bash
cargo run -p pedidos-rust
```

**Restaurant:**

```bash
cargo run -p restaurant
```

**Rider:**

```bash
cargo run -p rider
```

---

## Diseño

### <ins>Application level</ins>

<p align="center">
    <img src="./docs/imgs/c4_app_level.png" alt="PedidosRust logo" height="800px">
</p>

### <ins>Pedidos-rust</ins>

#### Finalidad

Aplicación que es un servidor distribuido que recibe pedidos de los customers, autoriza los pagos con el payment gateway, coordina con los restaurantes la preparación del pedido y una vez listo le ofrece los pedidos a los riders para que realicen el delivery y le da la asignación final al que acepte primero, mantiene actualizado al customer del estado del pedido en todo momento y efectiviza el cobro del pedido una vez se confirma que el delivery fue entregado al customer.

#### Estado interno

La estructura de la app consta de una entidad `Server` que es dueña del welcomming socket de TCP (usado de manera asíncrona para recibir conexiones), por cada conexión recibida de tiene un actor `ClientConnection` y estos pueden solicitar funcionalidades que requieren visibilidad de todas las conexiones del actor `ConnectionManager`. Todo el modelado de actores apalanca los handlers asíncronos para la concurrencia.

<p align="center">
    <img src="./docs/imgs/pedidos_rust_app.png" alt="pedidos_rust_app" height="500px">
</p>

**Variables internas de `Server`:**

```rust
pub struct Server {
    logger: Logger,
    connection_manager: Addr<ConnectionManager>,
}
```

**Variables internas de `ClientConnection`:**

```rust
pub struct ClientConnection {
    pub tcp_sender: Addr<TcpSender>,
    pub logger: Logger,
    pub id: u32,
    pub connection_manager: Addr<ConnectionManager>,
    pub peer_location: Option<Location>,
}
```

**Variables internas de `ConnectionManager`:**

```rust
pub struct ConnectionManager {
    pub logger: Logger,
    pub riders: HashMap<RiderId, Addr<ClientConnection>>,
    pub customers: HashMap<CustomerId, CustomerData>,
    pub orders_in_process: HashMap<RiderId, CustomerId>,
    pub pending_delivery_requests: VecDeque<Message>,
}
```

### <ins>Customer</ins>

#### Finalidad

Aplicación que utiliza el customer para poder consultar los restaurantes donde puede hacer un pedido, elegir un restaurante para realizarle un pedido y recibir notificaciones (push notifications) de cómo progresa el pedido a medida que se avanza en el proceso de delivery.

Proceso de delivery:

1. Pago autorizado
1. Restaurante está preparando el pedido
1. Pedido está listo
1. Rider 1234 está llevando el pedido
1. Pedido entregado

#### Estado interno

Está modelado como un actor con handlers asíncronos para manejar la concurrencia.

**Variables internas de `Customer`:**

```rust
struct Customer {
    tcp_sender: Addr<TcpSender>,
    logger: Logger,
    location: Location,
}
```

### <ins>Rider</ins>

#### Finalidad

Aplicación que utiliza el rider para actualizar su ubicación al PedidosRust, recibir ofertas de deliveries (se le dice *oferta* porque PedidosRust ofrece a los riders la posibilidad de realizar un delivery, el rider puede aceptar o no, y PedidosRust confirma si el rider fue elegido como el que efectivamente va a realizar el envío), ir a retirar el pedido del restaurante para llevarle al customer, y viajar hasta la ubicación del customer para realizar la entrega del pedido.

#### Estado interno

Está modelado como un actor con handlers asíncronos para manejar la concurrencia.

**Variables internas de `Rider`:**

```rust
struct Rider {
    tcp_sender: Addr<TcpSender>,
    logger: Logger,
    location: Location,
    customer_location: Option<Location>,
    busy: bool,
}
```

### <ins>Restaurant</ins>

#### Finalidad

#### Estado interno



### <ins>Payment</ins>

#### Finalidad

#### Estado interno



### <ins>Mensajes</ins>

#### Finalidad

#### Estado interno


