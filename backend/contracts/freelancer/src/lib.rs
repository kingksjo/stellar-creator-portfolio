#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String};

#[contracttype]
pub struct FreelancerProfile {
    pub address: Address,
    pub name: String,
    pub discipline: String,
    pub bio: String,
    pub rating: u32,
    pub total_rating_count: u32,
    pub completed_projects: u32,
    pub total_earnings: i128,
    pub verified: bool,
    pub created_at: u64,
}

#[contracttype]
pub enum DataKey {
    FreelancerCount,
    Profile(Address),
}

#[contract]
pub struct FreelancerContract;

#[contractimpl]
impl FreelancerContract {
    /// Registers a new freelancer profile.
    ///
    /// # Parameters
    /// - `env`: Soroban environment.
    /// - `freelancer`: Freelancer address (must authenticate).
    /// - `name`: Freelancer's display name.
    /// - `discipline`: Area of expertise (e.g., \"Rust Development\").
    /// - `bio`: Professional bio/description.
    ///
    /// # Returns
    /// - `bool`: `true` if registration succeeded, `false` if already registered.
    ///
    /// # Errors
    /// - Panics if freelancer fails authentication.
    ///
    /// # State Changes
    /// - Creates new FreelancerProfile with default metrics.
    /// - Increments freelancer count.
    pub fn register_freelancer(
        env: Env,
        freelancer: Address,
        name: String,
        discipline: String,
        bio: String,
    ) -> bool {
        freelancer.require_auth();

        let key = DataKey::Profile(freelancer.clone());
        if env.storage().persistent().has(&key) {
            return false;
        }

        let profile = FreelancerProfile {
            address: freelancer,
            name,
            discipline,
            bio,
            rating: 0,
            total_rating_count: 0,
            completed_projects: 0,
            total_earnings: 0,
            verified: false,
            created_at: env.ledger().timestamp(),
        };

        env.storage().persistent().set(&key, &profile);

        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::FreelancerCount)
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::FreelancerCount, &(count + 1));

        true
    }

    /// Retrieves freelancer profile.
    ///
    /// # Parameters
    /// - `env`: Soroban environment.
    /// - `freelancer`: Freelancer address.
    ///
    /// # Returns
    /// - `FreelancerProfile`: Complete profile data.
    ///
    /// # Errors
    /// - Panics with \"Freelancer not registered\" if profile doesn't exist.
    pub fn get_profile(env: Env, freelancer: Address) -> FreelancerProfile {
        env.storage()
            .persistent()
            .get(&DataKey::Profile(freelancer))
            .expect(\"Freelancer not registered\")
    }

    /// Updates freelancer's average rating with new review.
    /// Uses running average calculation.
    ///
    /// # Parameters
    /// - `env`: Soroban environment.
    /// - `freelancer`: Target freelancer.
    /// - `new_rating`: New rating (0-5 expected).
    ///
    /// # Returns
    /// - `bool`: Always `true`.
    ///
    /// # Errors
    /// - Panics if freelancer not registered.
    ///
    /// # Logic
    /// - total = old_rating * count
    /// - new_avg = (total + new_rating) / (count + 1)
    pub fn update_rating(env: Env, freelancer: Address, new_rating: u32) -> bool {
        let key = DataKey::Profile(freelancer);
        let mut profile: FreelancerProfile = env
            .storage()
            .persistent()
            .get(&key)
            .expect(\"Freelancer not registered\");

        let total = (profile.rating as u64) * (profile.total_rating_count as u64);
        profile.total_rating_count += 1;
        profile.rating = ((total + new_rating as u64) / profile.total_rating_count as u64) as u32;

        env.storage().persistent().set(&key, &profile);
        true
    }

    /// Increments freelancer's completed projects count.
    ///
    /// # Parameters
    /// - `env`: Soroban environment.
    /// - `freelancer`: Target freelancer.
    ///
    /// # Returns
    /// - `bool`: Always `true`.
    ///
    /// # Errors
    /// - Panics if freelancer not registered.
    pub fn update_completed_projects(env: Env, freelancer: Address) -> bool {
        let key = DataKey::Profile(freelancer);
        let mut profile: FreelancerProfile = env
            .storage()
            .persistent()
            .get(&key)
            .expect(\"Freelancer not registered\");

        profile.completed_projects += 1;
        env.storage().persistent().set(&key, &profile);
        true
    }

    /// Adds to freelancer's total earnings.
    ///
    /// # Parameters
    /// - `env`: Soroban environment.
    /// - `freelancer`: Target freelancer.
    /// - `amount`: Earnings amount to add (positive).
    ///
    /// # Returns
    /// - `bool`: Always `true`.
    ///
    /// # Errors
    /// - Panics if freelancer not registered.
    pub fn update_earnings(env: Env, freelancer: Address, amount: i128) -> bool {
        let key = DataKey::Profile(freelancer);
        let mut profile: FreelancerProfile = env
            .storage()
            .persistent()
            .get(&key)
            .expect(\"Freelancer not registered\");

        profile.total_earnings += amount;
        env.storage().persistent().set(&key, &profile);
        true
    }

    /// Admin verifies freelancer (sets verified flag).
    ///
    /// # Parameters
    /// - `env`: Soroban environment.
    /// - `admin`: Admin address (must authenticate).
    /// - `freelancer`: Target freelancer.
    ///
    /// # Returns
    /// - `bool`: Always `true`.
    ///
    /// # Errors
    /// - Panics if admin fails authentication.
    /// - Panics if freelancer not registered.
    pub fn verify_freelancer(env: Env, admin: Address, freelancer: Address) -> bool {
        admin.require_auth();

        let key = DataKey::Profile(freelancer);
        let mut profile: FreelancerProfile = env
            .storage()
            .persistent()
            .get(&key)
            .expect(\"Freelancer not registered\");

        profile.verified = true;
        env.storage().persistent().set(&key, &profile);
        true
    }

    /// Checks if freelancer is verified.
    ///
    /// # Parameters
    /// - `env`: Soroban environment.
    /// - `freelancer`: Freelancer address.
    ///
    /// # Returns
    /// - `bool`: `true` if verified, `false` if not registered or unverified.
    pub fn is_verified(env: Env, freelancer: Address) -> bool {
        env.storage()
            .persistent()
            .get::<DataKey, FreelancerProfile>(&DataKey::Profile(freelancer))
            .map(|p| p.verified)
            .unwrap_or(false)
    }

    /// Gets total registered freelancers count.
    ///
    /// # Parameters
    /// - `env`: Soroban environment.
    ///
    /// # Returns
    /// - `u32`: Count of freelancers.
    pub fn get_freelancers_count(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::FreelancerCount)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    #[test]
    fn test_register_freelancer() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(FreelancerContract, ());
        let client = FreelancerContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let result = client.register_freelancer(
            &freelancer,
            &String::from_str(&env, \"Alice\"),
            &String::from_str(&env, \"UI/UX Design\"),
            &String::from_str(&env, \"Designer with 5 years experience\"),
        );

        assert!(result);
        assert_eq!(client.get_freelancers_count(), 1);
        assert!(!client.is_verified(&freelancer));
    }

    #[test]
    fn test_duplicate_registration_returns_false() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(FreelancerContract, ());
        let client = FreelancerContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        client.register_freelancer(
            &freelancer,
            &String::from_str(&env, \"Alice\"),
            &String::from_str(&env, \"Design\"),
            &String::from_str(&env, \"Bio\"),
        );
        let second = client.register_freelancer(
            &freelancer,
            &String::from_str(&env, \"Alice\"),
            &String::from_str(&env, \"Design\"),
            &String::from_str(&env, \"Bio\"),
        );
        assert!(!second);
    }
}
