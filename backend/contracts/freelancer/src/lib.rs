#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, String, Symbol, Vec};

#[contracttype]
#[derive(Clone, Debug)]
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
    pub skills: Vec<String>,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct FilterOptions {
    pub discipline: Option<String>,
    pub min_rating: Option<u32>,
    pub verified_only: Option<bool>,
    pub skill: Option<String>,
}

#[contracttype]
pub enum DataKey {
    FreelancerCount,
    Profile(Address),
    AllFreelancers,
}

#[contract]
pub struct FreelancerContract;

const FL: Symbol = symbol_short!("fl"); 

#[contractimpl]
impl FreelancerContract {
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

        let timestamp = env.ledger().timestamp();
        let profile = FreelancerProfile {
            address: freelancer.clone(),
            name: name.clone(),
            discipline,
            bio,
            rating: 0,
            total_rating_count: 0,
            completed_projects: 0,
            total_earnings: 0,
            verified: false,
            created_at: timestamp,
            skills: Vec::new(&env),
        };

        env.storage().persistent().set(&key, &profile);

        let mut freelancers: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::AllFreelancers)
            .unwrap_or(Vec::new(&env));
        freelancers.push_back(freelancer.clone());
        env.storage().persistent().set(&DataKey::AllFreelancers, &freelancers);

        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::FreelancerCount)
            .unwrap_or(0);
        env.storage().persistent().set(&DataKey::FreelancerCount, &(count + 1));

        env.events().publish(
            (FL, symbol_short!("reg"), freelancer),
            (name, timestamp),
        );

        true
    }

    pub fn get_profile(env: Env, freelancer: Address) -> FreelancerProfile {
        env.storage()
            .persistent()
            .get(&DataKey::Profile(freelancer))
            .expect("not found")
    }

    pub fn update_rating(env: Env, freelancer: Address, new_rating: u32) -> bool {
        let key = DataKey::Profile(freelancer.clone());
        let mut profile: FreelancerProfile = env
            .storage()
            .persistent()
            .get(&key)
            .expect("not found");

        let total = (profile.rating as u64) * (profile.total_rating_count as u64);
        profile.total_rating_count += 1;
        profile.rating = ((total + new_rating as u64) / profile.total_rating_count as u64) as u32;

        env.storage().persistent().set(&key, &profile);

        env.events().publish(
            (FL, symbol_short!("rate"), freelancer),
            (profile.rating, profile.total_rating_count),
        );

        true
    }

    pub fn verify_freelancer(env: Env, admin: Address, freelancer: Address) -> bool {
        // Require the caller to authenticate as the admin address passed in.
        admin.require_auth();

        // If a governance contract is configured, delegate the admin-role check to it.
        // This keeps verification meaningful: only addresses that the governance
        // contract recognizes as admins can verify freelancers. If no governance
        // contract is configured, fall back to the legacy behaviour (auth only).
        if let Some(gov) = env.storage().persistent().get::<DataKey, Address>(&DataKey::Governance) {
                // Call governance contract's `is_admin` entrypoint. If it returns
                // false, reject. We expect the governance contract to expose a
                // method named `is_admin` that takes an Address and returns bool.
                // If the governance contract is not present or doesn't expose the
                // method, this will trap at runtime — that's intentional to make
                // misconfiguration visible.
                let is_admin: bool = env.invoke_contract(&gov, &symbol_short!("is_admin"), (admin.clone(),));
                if !is_admin {
                    panic!("Admin role required");
                }
            }
        }

        let key = DataKey::Profile(freelancer.clone());
        let mut profile: FreelancerProfile = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Freelancer not registered");

        profile.verified = true;
        env.storage().persistent().set(&key, &profile);

        env.events().publish(
            (FL, symbol_short!("ver"), freelancer),
            (admin, true),
        );

        true
    }

    /// Sets the governance contract address used for admin role checks.
    /// Can be called by any address that authenticates; this is intentionally
    /// permissive to allow initial configuration. Operators should set this
    /// to the governance contract address and then manage admin roles via the
    /// governance contract itself.
    pub fn set_governance_contract(env: Env, setter: Address, governance: Address) -> bool {
        setter.require_auth();

        // If deployer not set yet, record the first setter as the deployer.
        let maybe_deployer: Option<Address> = env.storage().persistent().get(&DataKey::Deployer);
        if let Some(deployer) = maybe_deployer {
            // Only the recorded deployer can change the governance address
            if deployer != setter {
                panic!("Only deployer may set governance contract");
            }
        } else {
            // Record the setter as deployer on first-time configuration
            env.storage().persistent().set(&DataKey::Deployer, &setter);
        }

        env.storage()
            .persistent()
            .set(&DataKey::Governance, &governance);
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

    pub fn get_freelancers_count(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::FreelancerCount)
            .unwrap_or(0)
    }

    pub fn add_skill(env: Env, freelancer: Address, skill: String) -> bool {
        freelancer.require_auth();
        let key = DataKey::Profile(freelancer.clone());
        let mut profile: FreelancerProfile = env
            .storage()
            .persistent()
            .get(&key)
            .expect("not found");

        for s in profile.skills.iter() {
            if s == skill {
                return false;
            }
        }

        profile.skills.push_back(skill.clone());
        env.storage().persistent().set(&key, &profile);

        env.events().publish(
            (FL, symbol_short!("sk_add"), freelancer),
            skill,
        );
        true
    }

    pub fn remove_skill(env: Env, freelancer: Address, skill: String) -> bool {
        freelancer.require_auth();
        let key = DataKey::Profile(freelancer.clone());
        let mut profile: FreelancerProfile = env
            .storage()
            .persistent()
            .get(&key)
            .expect("not found");

        let mut index = None;
        for (i, s) in profile.skills.iter().enumerate() {
            if s == skill {
                index = Some(i as u32);
                break;
            }
        }

        if let Some(i) = index {
            profile.skills.remove(i);
            env.storage().persistent().set(&key, &profile);
            env.events().publish(
                (FL, symbol_short!("sk_rem"), freelancer),
                skill,
            );
            true
        } else {
            false
        }
    }

    pub fn query_freelancers(env: Env, filters: FilterOptions) -> Vec<FreelancerProfile> {
        let freelancers: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::AllFreelancers)
            .unwrap_or(Vec::new(&env));
        let mut result = Vec::new(&env);

        for freelancer in freelancers.iter() {
            if let Some(profile) = env
                .storage()
                .persistent()
                .get::<DataKey, FreelancerProfile>(&DataKey::Profile(freelancer))
            {
                if let Some(ref discipline) = filters.discipline {
                    if profile.discipline != *discipline { continue; }
                }
                if let Some(min_rating) = filters.min_rating {
                    if profile.rating < min_rating { continue; }
                }
                if let Some(verified_only) = filters.verified_only {
                    if verified_only && !profile.verified { continue; }
                }
                if let Some(ref skill) = filters.skill {
                    let mut has_skill = false;
                    for s in profile.skills.iter() {
                        if s == *skill { has_skill = true; break; }
                    }
                    if !has_skill { continue; }
                }
                result.push_back(profile);
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{Env, testutils::Address as _};

    #[test]
    fn test_full_workflow() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(FreelancerContract, ());
        let client = FreelancerContractClient::new(&env, &contract_id);
        let freelancer = Address::generate(&env);

        // Register
        client.register_freelancer(&freelancer, &String::from_str(&env, "Alice"), &String::from_str(&env, "Design"), &String::from_str(&env, "Bio"));
        
        // Add skill
        let skill = String::from_str(&env, "Rust");
        client.add_skill(&freelancer, &skill);
        assert_eq!(client.get_profile(&freelancer).skills.len(), 1);

        // Update rating
        client.update_rating(&freelancer, &5);
        assert_eq!(client.get_profile(&freelancer).rating, 5);

        // Verify
        let admin = Address::generate(&env);
        client.verify_freelancer(&admin, &freelancer);
        assert!(client.is_verified(&freelancer));

        // Query
        let filters = FilterOptions {
            discipline: None,
            min_rating: Some(4),
            verified_only: Some(true),
            skill: Some(skill),
        };
        let result = client.query_freelancers(&filters);
        assert_eq!(result.len(), 1);
        
        // Remove skill
        client.remove_skill(&freelancer, &String::from_str(&env, "Rust"));
        assert_eq!(client.get_profile(&freelancer).skills.len(), 0);
    }
}
