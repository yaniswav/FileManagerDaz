//! Intelligent destination module
//!
//! Proposes a fine-grained destination based on DAZ content analysis.

use crate::config::SETTINGS;
use crate::core::analyzer::{AnalysisSummary, ContentType};
use crate::error::AppResult;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::info;

/// Destination proposal for an import
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationProposal {
    /// Target library (root path)
    pub library_path: String,
    /// Proposed relative subfolder (e.g.: "People/Genesis 9/Characters")
    pub relative_path: String,
    /// Full path
    pub full_path: String,
    /// Reason for the proposal
    pub reason: String,
    /// Confidence (0-100)
    pub confidence: u8,
    /// Other alternative proposals
    pub alternatives: Vec<DestinationAlternative>,
}

/// A destination alternative
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationAlternative {
    pub relative_path: String,
    pub full_path: String,
    pub reason: String,
}

/// Rules to determine the destination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DestinationRule {
    /// Rule name
    pub name: String,
    /// Target content type
    pub content_type: Option<ContentType>,
    /// Target figure (e.g.: "Genesis 9")
    pub target_figure: Option<String>,
    /// Tags that trigger the rule
    pub trigger_tags: Vec<String>,
    /// Destination relative path
    pub destination: String,
    /// Priority (higher = priority)
    pub priority: u8,
}

impl Default for DestinationRule {
    fn default() -> Self {
        Self {
            name: String::new(),
            content_type: None,
            target_figure: None,
            trigger_tags: Vec::new(),
            destination: String::new(),
            priority: 0,
        }
    }
}

/// Default rules for DAZ Studio
pub fn default_rules() -> Vec<DestinationRule> {
    vec![
        // Genesis 9 Characters
        DestinationRule {
            name: "Genesis 9 Female Character".into(),
            content_type: Some(ContentType::Character),
            target_figure: Some("Genesis 9".into()),
            trigger_tags: vec!["female".into()],
            destination: "People/Genesis 9/Characters/Female".into(),
            priority: 90,
        },
        DestinationRule {
            name: "Genesis 9 Male Character".into(),
            content_type: Some(ContentType::Character),
            target_figure: Some("Genesis 9".into()),
            trigger_tags: vec!["male".into()],
            destination: "People/Genesis 9/Characters/Male".into(),
            priority: 90,
        },
        DestinationRule {
            name: "Genesis 9 Character".into(),
            content_type: Some(ContentType::Character),
            target_figure: Some("Genesis 9".into()),
            trigger_tags: vec![],
            destination: "People/Genesis 9/Characters".into(),
            priority: 80,
        },
        // Genesis 8 Characters
        DestinationRule {
            name: "Genesis 8 Female Character".into(),
            content_type: Some(ContentType::Character),
            target_figure: Some("Genesis 8 Female".into()),
            trigger_tags: vec![],
            destination: "People/Genesis 8 Female/Characters".into(),
            priority: 85,
        },
        DestinationRule {
            name: "Genesis 8 Male Character".into(),
            content_type: Some(ContentType::Character),
            target_figure: Some("Genesis 8 Male".into()),
            trigger_tags: vec![],
            destination: "People/Genesis 8 Male/Characters".into(),
            priority: 85,
        },
        DestinationRule {
            name: "Genesis 8.1 Female Character".into(),
            content_type: Some(ContentType::Character),
            target_figure: Some("Genesis 8.1 Female".into()),
            trigger_tags: vec![],
            destination: "People/Genesis 8.1 Female/Characters".into(),
            priority: 86,
        },
        // Cheveux
        DestinationRule {
            name: "Genesis 9 Hair".into(),
            content_type: Some(ContentType::Hair),
            target_figure: Some("Genesis 9".into()),
            trigger_tags: vec![],
            destination: "People/Genesis 9/Hair".into(),
            priority: 85,
        },
        DestinationRule {
            name: "Genesis 8 Female Hair".into(),
            content_type: Some(ContentType::Hair),
            target_figure: Some("Genesis 8 Female".into()),
            trigger_tags: vec![],
            destination: "People/Genesis 8 Female/Hair".into(),
            priority: 84,
        },
        // Clothing
        DestinationRule {
            name: "Genesis 9 Clothing".into(),
            content_type: Some(ContentType::Clothing),
            target_figure: Some("Genesis 9".into()),
            trigger_tags: vec![],
            destination: "People/Genesis 9/Clothing".into(),
            priority: 85,
        },
        DestinationRule {
            name: "Genesis 8 Female Clothing".into(),
            content_type: Some(ContentType::Clothing),
            target_figure: Some("Genesis 8 Female".into()),
            trigger_tags: vec![],
            destination: "People/Genesis 8 Female/Clothing".into(),
            priority: 84,
        },
        // Poses
        DestinationRule {
            name: "Genesis 9 Pose".into(),
            content_type: Some(ContentType::Pose),
            target_figure: Some("Genesis 9".into()),
            trigger_tags: vec![],
            destination: "People/Genesis 9/Poses".into(),
            priority: 85,
        },
        // Morphs
        DestinationRule {
            name: "Genesis 9 Morph".into(),
            content_type: Some(ContentType::Morph),
            target_figure: Some("Genesis 9".into()),
            trigger_tags: vec![],
            destination: "data/DAZ 3D/Genesis 9/Morphs".into(),
            priority: 90,
        },
        DestinationRule {
            name: "Genesis 8 Female Morph".into(),
            content_type: Some(ContentType::Morph),
            target_figure: Some("Genesis 8 Female".into()),
            trigger_tags: vec![],
            destination: "data/DAZ 3D/Genesis 8 Female/Morphs".into(),
            priority: 89,
        },
        // Props
        DestinationRule {
            name: "Prop".into(),
            content_type: Some(ContentType::Prop),
            target_figure: None,
            trigger_tags: vec![],
            destination: "Props".into(),
            priority: 50,
        },
        // Environnements
        DestinationRule {
            name: "Environment".into(),
            content_type: Some(ContentType::Environment),
            target_figure: None,
            trigger_tags: vec![],
            destination: "Environments".into(),
            priority: 50,
        },
        // Lights
        DestinationRule {
            name: "Light".into(),
            content_type: Some(ContentType::Light),
            target_figure: None,
            trigger_tags: vec![],
            destination: "Light Presets".into(),
            priority: 50,
        },
        // Materials
        DestinationRule {
            name: "Material".into(),
            content_type: Some(ContentType::Material),
            target_figure: None,
            trigger_tags: vec![],
            destination: "Shader Presets".into(),
            priority: 50,
        },
        // Scripts
        DestinationRule {
            name: "Script".into(),
            content_type: Some(ContentType::Script),
            target_figure: None,
            trigger_tags: vec![],
            destination: "Scripts".into(),
            priority: 50,
        },
    ]
}

/// Proposes a destination based on analysis
pub fn propose_destination(
    analysis: &AnalysisSummary,
    source_name: &str,
) -> AppResult<DestinationProposal> {
    let settings = SETTINGS
        .read()
        .map_err(|e| crate::error::AppError::Config(format!("Cannot read settings: {}", e)))?;

    // Get the default library
    let library_path = settings
        .default_destination
        .clone()
        .or_else(|| settings.daz_libraries.first().cloned())
        .unwrap_or_else(|| PathBuf::from(""));

    let library_str = library_path.to_string_lossy().to_string();

    drop(settings);

    let rules = default_rules();
    let mut matched_rules: Vec<(u8, &DestinationRule)> = Vec::new();

    // Evaluate each rule
    for rule in &rules {
        let mut score = 0u8;

        // Check content type
        if let Some(rule_type) = &rule.content_type {
            if *rule_type == analysis.content_type {
                score += 30;
            } else {
                continue; // Type doesn't match
            }
        }

        // Check target figure
        if let Some(ref target) = rule.target_figure {
            if analysis.detected_figures.iter().any(|f| f.contains(target)) {
                score += 40;
            } else if analysis.detected_figures.is_empty() {
                // No figure detected but rule requires a figure
                score += 5;
            } else {
                continue; // Figure doesn't match
            }
        }

        // Check tags
        if !rule.trigger_tags.is_empty() {
            let tag_matches = rule
                .trigger_tags
                .iter()
                .filter(|t| {
                    analysis
                        .suggested_tags
                        .iter()
                        .any(|st| st.to_lowercase().contains(&t.to_lowercase()))
                        || source_name.to_lowercase().contains(&t.to_lowercase())
                })
                .count();

            if tag_matches > 0 {
                score += (tag_matches * 10).min(30) as u8;
            }
        }

        if score > 0 {
            score += rule.priority;
            matched_rules.push((score, rule));
        }
    }

    // Sort by descending score
    matched_rules.sort_by(|a, b| b.0.cmp(&a.0));

    // Build proposal
    let (relative_path, reason, confidence) =
        if let Some((score, best_rule)) = matched_rules.first() {
            let conf = ((*score as u16 * 100) / 170).min(100) as u8;
            (
                best_rule.destination.clone(),
                format!("Rule \"{}\" applied", best_rule.name),
                conf,
            )
        } else {
            // Fallback based on content type
            let (path, reason) = match analysis.content_type {
                ContentType::Character => ("People", "Type: Character"),
                ContentType::Hair => ("Hair", "Type: Hair"),
                ContentType::Clothing => ("Clothing", "Type: Clothing"),
                ContentType::Prop => ("Props", "Type: Prop"),
                ContentType::Environment => ("Environments", "Type: Environment"),
                ContentType::Pose => ("Poses", "Type: Pose"),
                ContentType::Light => ("Light Presets", "Type: Light"),
                ContentType::Material => ("Shader Presets", "Type: Material"),
                ContentType::Script => ("Scripts", "Type: Script"),
                ContentType::Morph => ("data", "Type: Morph"),
                _ => ("", "No applicable rule"),
            };
            (path.to_string(), reason.to_string(), 30)
        };

    let full_path = if library_path.as_os_str().is_empty() {
        relative_path.clone()
    } else {
        library_path
            .join(&relative_path)
            .to_string_lossy()
            .to_string()
    };

    // Build alternatives
    let alternatives: Vec<DestinationAlternative> = matched_rules
        .iter()
        .skip(1)
        .take(3)
        .map(|(_, rule)| {
            let alt_full = if library_path.as_os_str().is_empty() {
                rule.destination.clone()
            } else {
                library_path
                    .join(&rule.destination)
                    .to_string_lossy()
                    .to_string()
            };
            DestinationAlternative {
                relative_path: rule.destination.clone(),
                full_path: alt_full,
                reason: rule.name.clone(),
            }
        })
        .collect();

    info!(
        "Proposed destination for '{}': {} (confidence: {}%)",
        source_name, relative_path, confidence
    );

    Ok(DestinationProposal {
        library_path: library_str,
        relative_path,
        full_path,
        reason,
        confidence,
        alternatives,
    })
}

/// Builds the final destination path
#[allow(dead_code)]
pub fn build_final_destination(library_path: &Path, relative_path: &str) -> PathBuf {
    library_path.join(relative_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_rules_loaded() {
        let rules = default_rules();
        assert!(!rules.is_empty());
        assert!(rules.iter().any(|r| r.name.contains("Genesis 9")));
    }

    #[test]
    fn test_propose_destination_character() {
        let analysis = AnalysisSummary {
            is_daz_content: true,
            content_type: ContentType::Character,
            daz_folders: vec!["data".into(), "People".into()],
            wrapper_folder: None,
            daz_file_count: 10,
            texture_count: 20,
            suggested_tags: vec!["female".into()],
            detected_figures: vec!["Genesis 9".into()],
            warnings: vec![],
        };

        let proposal = propose_destination(&analysis, "Victoria 9").unwrap();
        assert!(proposal.relative_path.contains("Genesis 9"));
        assert!(
            proposal.relative_path.contains("Character")
                || proposal.relative_path.contains("Female")
        );
        assert!(proposal.confidence > 50);
    }
}
