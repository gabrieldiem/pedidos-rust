use crate::client_connection::ClientConnection;
use crate::connection_manager::{RestaurantData, RiderData};
use actix::Addr;
use common::constants::MAX_DISTANCE_RESTAURANTS;
use common::protocol::Location;
use std::collections::HashMap;

pub struct NearbyEntities {}

impl NearbyEntities {
    /// Calculates the Manhattan distance between two locations.
    ///
    /// # Arguments
    ///
    /// * `loc1` - Reference to the first location.
    /// * `loc2` - Reference to the second location.
    ///
    /// # Returns
    ///
    /// Returns the Manhattan distance as a `u32`.
    pub fn manhattan_distance(loc1: &Location, loc2: &Location) -> u32 {
        loc1.x.abs_diff(loc2.x) as u32 + loc1.y.abs_diff(loc2.y) as u32
    }

    /// Returns a list of names of restaurants near a customer's location.
    ///
    /// A restaurant is considered nearby if its Manhattan distance to the customer
    /// is less than or equal to `MAX_DISTANCE_RESTAURANTS`.
    ///
    /// # Arguments
    ///
    /// * `customer_loc` - Reference to the customer's location.
    /// * `restaurants` - Reference to the map of restaurants (`HashMap<String, RestaurantData>`).
    ///
    /// # Returns
    ///
    /// A vector with references to the names of nearby restaurants.
    pub fn nearby_restaurants<'a>(
        customer_loc: &Location,
        restaurants: &'a HashMap<String, RestaurantData>,
    ) -> Vec<&'a String> {
        restaurants
            .iter()
            .filter_map(|(name, data)| {
                if Self::manhattan_distance(customer_loc, &data.location)
                    <= MAX_DISTANCE_RESTAURANTS
                {
                    Some(name)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Returns a vector of addresses of the closest riders to a target location.
    ///
    /// The function calculates the Manhattan distance from each rider's location to the target location,
    /// sorts the riders by distance, and returns the addresses of the closest `n` riders.
    ///
    /// # Arguments
    ///
    /// * `target_location` - Reference to the target location.
    /// * `riders` - Reference to a map of riders (`HashMap<u32, RiderData>`).
    /// * `n` - The number of closest riders to return.
    ///
    /// # Returns
    ///
    /// A vector containing references to the addresses of the closest riders.
    pub fn closest_riders<'a>(
        target_location: &Location,
        riders: &'a HashMap<u32, RiderData>,
        rider_connections: &'a HashMap<u32, Addr<ClientConnection>>,
        n: usize,
    ) -> Vec<&'a actix::Addr<ClientConnection>> {
        let mut riders_with_distance: Vec<_> = riders
            .into_iter()
            .filter_map(|(rider_id, rider_data)| {
                rider_data.location.map(|loc| {
                    let dist = Self::manhattan_distance(&loc, target_location);
                    let rider_adress = rider_connections.get(rider_id).unwrap(); // confirmed to exist
                    (dist, rider_adress)
                })
            })
            .collect();

        riders_with_distance.sort_by_key(|(dist, _)| *dist);

        riders_with_distance
            .into_iter()
            .take(n)
            .map(|(_, addr)| addr)
            .collect()
    }
}
