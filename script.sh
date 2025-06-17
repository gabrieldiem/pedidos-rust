#!/bin/bash

# Levantar instancias de PedidosRust
for i in {1..4}; do
    gnome-terminal -- bash -c "cargo run -p pedidos-rust $i; exec bash"
    sleep 0.3
done

# Levantar sistema de pagos
gnome-terminal -- bash -c "cargo run -p payment-system; exec bash"
sleep 0.3

# Levantar restaurantes
for i in {1..4}; do
    gnome-terminal -- bash -c "cargo run -p restaurant $i; exec bash"
    sleep 0.3
done

# Levantar clientes
for i in {1..5}; do
    gnome-terminal -- bash -c "cargo run -p customer $i; exec bash"
    sleep 0.3
done

# Levantar riders
for i in {1..4}; do
    gnome-terminal -- bash -c "cargo run -p rider $i; exec bash"
    sleep 0.3
done
