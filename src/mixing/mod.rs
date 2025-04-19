use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use topological_sort::TopologicalSort;

const MAX_EFFECTS: usize = 8;

// Define our Effect and Substance types
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, Serialize, Deserialize, Ord, PartialOrd)]
pub enum Effect {
    AntiGravity,
    Athletic,
    Balding,
    BrightEyed,
    Calming,
    CalorieDense,
    Cyclopean,
    Disorienting,
    Electrifying,
    Energizing,
    Euphoric,
    Explosive,
    Focused,
    Foggy,
    Gingeritis,
    Glowing,
    Jennerising,
    Laxative,
    LongFaced,
    Munchies,
    Paranoia,
    Refreshing,
    Schizophrenia,
    Sedating,
    Shrinking,
    SeizureInducing,
    Slippery,
    Smelly,
    Sneaky,
    Spicy,
    Toxic,
    ThoughtProvoking,
    TropicThunder,
    Zombifying,
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
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rule {
    pub if_present: Vec<Effect>,
    pub if_not_present: Vec<Effect>,
    pub replace: Vec<(Effect, Effect)>,
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
    inherent_effects: BTreeMap<Substance, Vec<Effect>>,
    price_map: BTreeMap<Effect, f64>,
}

impl MixtureRules {
    pub fn apply(&self, substance: Substance, effects: &mut BTreeSet<Effect>) {
        let replacements = self.replacement_rules.get(&substance).unwrap();
        let inherent_effects = self.inherent_effects.get(&substance);

        for rule in replacements {
            // Check if all required effects are present
            let present_check = rule
                .if_present
                .iter()
                .all(|effect| effects.contains(effect));

            // Check if all excluded effects are absent
            let absent_check = rule
                .if_not_present
                .iter()
                .all(|effect| !effects.contains(effect));

            // If all conditions are met, apply the replacements
            if present_check && absent_check {
                for (from, to) in &rule.replace {
                    if effects.remove(from) {
                        effects.insert(*to);
                    }
                }
            }
        }

        let Some(inherent_effects) = inherent_effects else {
            return;
        };
        for effect in inherent_effects {
            if effects.len() >= MAX_EFFECTS {
                return;
            }
            effects.insert(*effect);
        }
    }

    pub fn price_multiplier<'a>(&self, effects: impl Iterator<Item = &'a Effect>) -> f64 {
        let base = 1.0;
        let mut multiplier = 0.;

        for effect in effects {
            multiplier += self.price_map.get(effect).copied().unwrap_or_default();
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
            .collect::<Vec<Effect>>();

        let if_not_present = rule_json
            .if_not_present
            .iter()
            .map(|s| string_to_effect(s))
            .collect::<Vec<Effect>>();

        // Parse the replacements
        let mut replace = Vec::new();
        for (from, to) in rule_json.replace.entries.iter() {
            replace.push((string_to_effect(from), string_to_effect(to)));
        }

        let rule = Rule {
            if_present,
            if_not_present,
            replace,
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
        let mut ts = TopologicalSort::<&[Effect]>::new();
        for rule in rules.iter() {
            ts.add_dependency(&*rule.if_not_present, &*rule.if_present);
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
            .map(|e| string_to_effect(e))
            .collect::<Vec<Effect>>();
        inherent_effects.insert(substance, effects);
    }

    // Convert effect price mapping
    let mut price_map = BTreeMap::new();
    for (effect_string, price_string) in &rules_file.effect_price {
        let effect = string_to_effect(effect_string);
        let price = price_string.parse::<f64>()?;
        price_map.insert(effect, price);
    }

    Ok(MixtureRules {
        replacement_rules,
        inherent_effects,
        price_map,
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
fn string_to_effect(s: &str) -> Effect {
    match s {
        "Ag" => Effect::AntiGravity,
        "At" => Effect::Athletic,
        "Ba" => Effect::Balding,
        "Be" => Effect::BrightEyed,
        "Ca" => Effect::Calming,
        "Cd" => Effect::CalorieDense,
        "Cy" => Effect::Cyclopean,
        "Di" => Effect::Disorienting,
        "El" => Effect::Electrifying,
        "En" => Effect::Energizing,
        "Eu" => Effect::Euphoric,
        "Ex" => Effect::Explosive,
        "Fc" => Effect::Focused,
        "Fo" => Effect::Foggy,
        "Gi" => Effect::Gingeritis,
        "Gl" => Effect::Glowing,
        "Je" => Effect::Jennerising,
        "La" => Effect::Laxative,
        "Lf" => Effect::LongFaced,
        "Mu" => Effect::Munchies,
        "Pa" => Effect::Paranoia,
        "Re" => Effect::Refreshing,
        "Sc" => Effect::Schizophrenia,
        "Se" => Effect::Sedating,
        "Sh" => Effect::Shrinking,
        "Si" => Effect::SeizureInducing,
        "Sl" => Effect::Slippery,
        "Sm" => Effect::Smelly,
        "Sn" => Effect::Sneaky,
        "Sp" => Effect::Spicy,
        "To" => Effect::Toxic,
        "Tp" => Effect::ThoughtProvoking,
        "Tt" => Effect::TropicThunder,
        "Zo" => Effect::Zombifying,
        _ => panic!("Unknown effect: {}", s),
    }
}

#[cfg(test)]
mod tests {
    use crate::mixing::{parse_rules_file, Effect, Substance};
    use std::collections::BTreeSet;
    use std::error::Error;

    #[test]
    fn test_regression_cocaine() -> Result<(), Box<dyn Error>> {
        let rules = parse_rules_file("sch1-mix-rules.json")?;

        let mut effects = BTreeSet::new();

        // First mix
        rules.apply(Substance::HorseSemen, &mut effects);
        assert_eq!(effects, [Effect::LongFaced].into());

        // Second mix
        rules.apply(Substance::Addy, &mut effects);
        assert_eq!(
            effects,
            [Effect::Electrifying, Effect::ThoughtProvoking].into()
        );

        // Third mix
        rules.apply(Substance::Battery, &mut effects);
        assert_eq!(
            effects,
            [
                Effect::Euphoric,
                Effect::ThoughtProvoking,
                Effect::BrightEyed
            ]
            .into()
        );

        // Fourth mix
        rules.apply(Substance::HorseSemen, &mut effects);
        assert_eq!(
            effects,
            [
                Effect::Electrifying,
                Effect::BrightEyed,
                Effect::LongFaced,
                Effect::Euphoric
            ]
            .into()
        );

        Ok(())
    }

    #[test]
    fn test_regression_cocaine2() -> Result<(), Box<dyn Error>> {
        let rules = parse_rules_file("sch1-mix-rules.json")?;

        let mut effects = BTreeSet::new();

        // First mix
        rules.apply(Substance::MegaBean, &mut effects);
        assert_eq!(effects, [Effect::Foggy].into());

        // Second mix
        rules.apply(Substance::Cuke, &mut effects);
        assert_eq!(effects, [Effect::Cyclopean, Effect::Energizing].into());

        // Third mix
        rules.apply(Substance::Banana, &mut effects);
        assert_eq!(
            effects,
            [
                Effect::Energizing,
                Effect::ThoughtProvoking,
                Effect::Gingeritis
            ]
            .into()
        );

        // Fourth mix
        rules.apply(Substance::HorseSemen, &mut effects);
        assert_eq!(
            effects,
            [
                Effect::Energizing,
                Effect::Electrifying,
                Effect::Refreshing,
                Effect::LongFaced
            ]
            .into()
        );

        // Fifth mix
        rules.apply(Substance::Iodine, &mut effects);
        assert_eq!(
            effects,
            [
                Effect::Energizing,
                Effect::Electrifying,
                Effect::ThoughtProvoking,
                Effect::LongFaced,
                Effect::Jennerising
            ]
            .into()
        );

        Ok(())
    }

    #[test]
    fn test_price_regression() -> Result<(), Box<dyn Error>> {
        let rules = parse_rules_file("sch1-mix-rules.json")?;
        let effects: BTreeSet<_> = [
            Effect::AntiGravity,
            Effect::Glowing,
            Effect::TropicThunder,
            Effect::Zombifying,
            Effect::Cyclopean,
            Effect::Foggy,
            Effect::BrightEyed,
        ]
        .into();
        let price = (150.0 * rules.price_multiplier(effects.iter())).round() as i64;
        assert_eq!(price, 657);

        Ok(())
    }
}
