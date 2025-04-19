use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use topological_sort::TopologicalSort;

const MAX_EFFECTS: u32 = 8;
const NUM_EFFECTS: usize = 34;

bitflags! {
    #[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Copy, Clone, Hash)]
    pub struct Effects: u64 {
        const AntiGravity = 1 << 0;
        const Athletic = 1 << 1;
        const Balding = 1 << 2;
        const BrightEyed = 1 << 3;
        const Calming = 1 << 4;
        const CalorieDense = 1 << 5;
        const Cyclopean = 1 << 6;
        const Disorienting = 1 << 7;
        const Electrifying = 1 << 8;
        const Energizing = 1 << 9;
        const Euphoric = 1 << 10;
        const Explosive = 1 << 11;
        const Focused = 1 << 12;
        const Foggy = 1 << 13;
        const Gingeritis = 1 << 14;
        const Glowing = 1 << 15;
        const Jennerising = 1 << 16;
        const Laxative = 1 << 17;
        const LongFaced = 1 << 18;
        const Munchies = 1 << 19;
        const Paranoia = 1 << 20;
        const Refreshing = 1 << 21;
        const Schizophrenia = 1 << 22;
        const Sedating = 1 << 23;
        const Shrinking = 1 << 24;
        const SeizureInducing = 1 << 25;
        const Slippery = 1 << 26;
        const Smelly = 1 << 27;
        const Sneaky = 1 << 28;
        const Spicy = 1 << 29;
        const Toxic = 1 << 30;
        const ThoughtProvoking = 1 << 31;
        const TropicThunder = 1 << 32;
        const Zombifying = 1 << 33;
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, Serialize, Deserialize, Ord, PartialOrd)]
pub enum Substance {
    Cuke,
    FluMedicine,
    Gasoline,
    Donut,
    EnergyDrink,
    MouthWash,
    MotorOil,
    Banana,
    Chili,
    Iodine,
    Paracetamol,
    Viagra,
    HorseSemen,
    MegaBean,
    Addy,
    Battery,
}

pub const SUBSTANCES: &[Substance] = &[
    Substance::Cuke,
    Substance::FluMedicine,
    Substance::Gasoline,
    Substance::Donut,
    Substance::EnergyDrink,
    Substance::MouthWash,
    Substance::MotorOil,
    Substance::Banana,
    Substance::Chili,
    Substance::Iodine,
    Substance::Paracetamol,
    Substance::Viagra,
    Substance::HorseSemen,
    Substance::MegaBean,
    Substance::Addy,
    Substance::Battery,
];

// Define our Rule structure
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rule {
    pub if_present: Effects,
    pub if_not_present: Effects,
    pub remove: Effects,
    pub add: Effects,
}

// JSON input structures for deserialization
#[derive(Deserialize)]
struct ReplaceMap {
    #[serde(flatten)]
    entries: HashMap<String, String>,
}

#[derive(Deserialize)]
struct RuleJson {
    if_present: Vec<String>,
    if_not_present: Vec<String>,
    requires_substance: String,
    replace: ReplaceMap,
}

#[derive(Deserialize)]
struct EffectJson {
    substance: String,
    effect: Vec<String>,
}

#[derive(Deserialize)]
struct RulesFile {
    effects: Vec<EffectJson>,
    rules: Vec<RuleJson>,
    effect_price: HashMap<String, String>,
}

pub struct MixtureRules {
    replacement_rules: BTreeMap<Substance, Vec<Rule>>,
    inherent_effects: BTreeMap<Substance, Effects>,
    price_mults: [f64; NUM_EFFECTS],
}

impl MixtureRules {
    pub fn apply(&self, substance: Substance, effects: &mut Effects) {
        let replacements = self.replacement_rules.get(&substance).unwrap();
        let inherent_effects = self
            .inherent_effects
            .get(&substance)
            .copied()
            .unwrap_or(Effects::empty());

        for rule in replacements {
            if effects.contains(rule.if_present) && !effects.contains(rule.if_not_present) {
                effects.remove(rule.remove);
                effects.insert(rule.add);
            }
        }

        let n_effects = effects.bits().count_ones();
        if n_effects < MAX_EFFECTS {
            effects.insert(inherent_effects);
        }
    }

    pub fn price_multiplier(&self, effects: Effects) -> f64 {
        let base = 1.0;
        let mut multiplier = 0.;

        for effect in effects {
            let idx = effect.bits().ilog2();
            multiplier += self.price_mults[idx as usize];
        }

        base + multiplier
    }
}

// Function to parse JSON file into a HashMap of Substance to Rules
pub fn parse_rules_file<P: AsRef<Path>>(
    path: P,
) -> Result<MixtureRules, Box<dyn std::error::Error>> {
    // Open the file
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    // Parse the JSON
    let rules_file: RulesFile = serde_json::from_reader(reader)?;

    // Convert to our internal representation
    let mut replacement_rules = BTreeMap::new();

    for rule_json in rules_file.rules {
        let substance = match string_to_substance(&rule_json.requires_substance) {
            Some(value) => value,
            None => continue,
        };

        // Parse the effects
        let if_present = rule_json
            .if_present
            .iter()
            .map(|s| string_to_effect(s))
            .fold(Effects::empty(), |a, b| a | b);

        let if_not_present = rule_json
            .if_not_present
            .iter()
            .map(|s| string_to_effect(s))
            .fold(Effects::empty(), |a, b| a | b);

        // Parse the replacements
        let mut remove = Effects::empty();
        let mut add = Effects::empty();
        for (from, to) in rule_json.replace.entries.iter() {
            remove |= string_to_effect(from);
            add |= string_to_effect(to);
        }

        let rule = Rule {
            if_present,
            if_not_present,
            remove,
            add,
        };

        // Add to our HashMap
        replacement_rules
            .entry(substance)
            .or_insert_with(Vec::new)
            .push(rule);
    }

    // Topo sort the replacement rules
    // if {A -> B, B -> C} is applied to {A, B}, should end up with {B, C}
    for rules in replacement_rules.values_mut() {
        let mut ts = TopologicalSort::<Effects>::new();
        for rule in rules.iter() {
            ts.add_dependency(rule.if_not_present, rule.if_present);
        }
        let mut new_order = Vec::with_capacity(rules.len());
        for effects in ts {
            if let Some(r) = rules.iter().find(|r| r.if_present == effects) {
                new_order.push(r.clone());
            }
        }
        *rules = new_order;
    }

    // Convert inherent effects
    let mut inherent_effects = BTreeMap::new();
    for effect_json in &rules_file.effects {
        let substance = string_to_substance(&effect_json.substance).unwrap();
        let effects = effect_json
            .effect
            .iter()
            .map(|s| string_to_effect(s))
            .fold(Effects::empty(), |a, b| a | b);
        inherent_effects.insert(substance, effects);
    }

    // Convert effect price mapping
    let mut price_mults = [0.; NUM_EFFECTS];
    for (effect_string, price_string) in &rules_file.effect_price {
        let effect = string_to_effect(effect_string);
        let idx = effect.bits().ilog2();
        let price = price_string.parse::<f64>()?;
        price_mults[idx as usize] = price;
    }

    Ok(MixtureRules {
        replacement_rules,
        inherent_effects,
        price_mults,
    })
}

fn string_to_substance(substance: &str) -> Option<Substance> {
    // Parse the substance
    let substance = match substance {
        "A" => Substance::Cuke,
        "B" => Substance::FluMedicine,
        "C" => Substance::Gasoline,
        "D" => Substance::Donut,
        "E" => Substance::EnergyDrink,
        "F" => Substance::MouthWash,
        "G" => Substance::MotorOil,
        "H" => Substance::Banana,
        "I" => Substance::Chili,
        "J" => Substance::Iodine,
        "K" => Substance::Paracetamol,
        "L" => Substance::Viagra,
        "M" => Substance::HorseSemen,
        "N" => Substance::MegaBean,
        "O" => Substance::Addy,
        "P" => Substance::Battery,
        _ => return None, // Skip invalid substances
    };
    Some(substance)
}

// Helper function to convert string to Effect enum
fn string_to_effect(s: &str) -> Effects {
    match s {
        "Ag" => Effects::AntiGravity,
        "At" => Effects::Athletic,
        "Ba" => Effects::Balding,
        "Be" => Effects::BrightEyed,
        "Ca" => Effects::Calming,
        "Cd" => Effects::CalorieDense,
        "Cy" => Effects::Cyclopean,
        "Di" => Effects::Disorienting,
        "El" => Effects::Electrifying,
        "En" => Effects::Energizing,
        "Eu" => Effects::Euphoric,
        "Ex" => Effects::Explosive,
        "Fc" => Effects::Focused,
        "Fo" => Effects::Foggy,
        "Gi" => Effects::Gingeritis,
        "Gl" => Effects::Glowing,
        "Je" => Effects::Jennerising,
        "La" => Effects::Laxative,
        "Lf" => Effects::LongFaced,
        "Mu" => Effects::Munchies,
        "Pa" => Effects::Paranoia,
        "Re" => Effects::Refreshing,
        "Sc" => Effects::Schizophrenia,
        "Se" => Effects::Sedating,
        "Sh" => Effects::Shrinking,
        "Si" => Effects::SeizureInducing,
        "Sl" => Effects::Slippery,
        "Sm" => Effects::Smelly,
        "Sn" => Effects::Sneaky,
        "Sp" => Effects::Spicy,
        "To" => Effects::Toxic,
        "Tp" => Effects::ThoughtProvoking,
        "Tt" => Effects::TropicThunder,
        "Zo" => Effects::Zombifying,
        _ => panic!("Unknown effect: {}", s),
    }
}

#[cfg(test)]
mod tests {
    use crate::mixing::{parse_rules_file, Effects, Substance};
    use std::error::Error;

    #[test]
    fn test_regression_cocaine() -> Result<(), Box<dyn Error>> {
        let rules = parse_rules_file("sch1-mix-rules.json")?;

        let mut effects = Effects::empty();

        // First mix
        rules.apply(Substance::HorseSemen, &mut effects);
        assert_eq!(effects, Effects::LongFaced);

        // Second mix
        rules.apply(Substance::Addy, &mut effects);
        assert_eq!(effects, Effects::Electrifying | Effects::ThoughtProvoking);

        // Third mix
        rules.apply(Substance::Battery, &mut effects);
        assert_eq!(
            effects,
            Effects::Euphoric | Effects::ThoughtProvoking | Effects::BrightEyed
        );

        // Fourth mix
        rules.apply(Substance::HorseSemen, &mut effects);
        assert_eq!(
            effects,
            Effects::Electrifying | Effects::BrightEyed | Effects::LongFaced | Effects::Euphoric
        );

        Ok(())
    }

    #[test]
    fn test_regression_cocaine2() -> Result<(), Box<dyn Error>> {
        let rules = parse_rules_file("sch1-mix-rules.json")?;

        let mut effects = Effects::empty();

        // First mix
        rules.apply(Substance::MegaBean, &mut effects);
        assert_eq!(effects, Effects::Foggy);

        // Second mix
        rules.apply(Substance::Cuke, &mut effects);
        assert_eq!(effects, Effects::Cyclopean | Effects::Energizing);

        // Third mix
        rules.apply(Substance::Banana, &mut effects);
        assert_eq!(
            effects,
            Effects::Energizing | Effects::ThoughtProvoking | Effects::Gingeritis
        );

        // Fourth mix
        rules.apply(Substance::HorseSemen, &mut effects);
        assert_eq!(
            effects,
            Effects::Energizing | Effects::Electrifying | Effects::Refreshing | Effects::LongFaced
        );

        // Fifth mix
        rules.apply(Substance::Iodine, &mut effects);
        assert_eq!(
            effects,
            Effects::Energizing
                | Effects::Electrifying
                | Effects::ThoughtProvoking
                | Effects::LongFaced
                | Effects::Jennerising
        );

        Ok(())
    }

    #[test]
    fn test_price_regression() -> Result<(), Box<dyn Error>> {
        let rules = parse_rules_file("sch1-mix-rules.json")?;
        let effects = Effects::AntiGravity
            | Effects::Glowing
            | Effects::TropicThunder
            | Effects::Zombifying
            | Effects::Cyclopean
            | Effects::Foggy
            | Effects::BrightEyed;
        let price = (150.0 * rules.price_multiplier(effects)).round() as i64;
        assert_eq!(price, 657);

        Ok(())
    }
}
