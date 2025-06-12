use crate::connection_manager::RestaurantData;
use common::constants::MAX_DISTANCE_RESTAURANTS;
use common::protocol::Location;
use std::collections::HashMap;

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
            if manhattan_distance(customer_loc, &data.location) <= MAX_DISTANCE_RESTAURANTS {
                Some(name)
            } else {
                None
            }
        })
        .collect()
}
