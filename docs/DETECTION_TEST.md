# Test: Detection automatique des archives déjà extraites

## Situation

Avant le patch, tu avais:
- **538 archives extraites** → Dossiers `*_extracted` dans temp
- **2785 archives restantes** → Non traitées

## Solution

Le système détecte maintenant automatiquement les archives déjà extraites **AVANT** le nettoyage:

```rust
// 1. DÉTECTION (avant cleanup)
let already_extracted = detect_already_extracted(&temp_dir, &paths);
// → Trouve 538 dossiers *_extracted et match avec les paths sources

// 2. CLEANUP (supprime les _extracted)
cleanup_extracted_folders(&temp_dir);
// → Libère l'espace disque

// 3. CHECKPOINT (marque comme traités)
for extracted_path in &already_extracted {
    checkpoint.mark_processed(extracted_path);
}
// → 538 archives marquées comme "déjà faites"

// 4. FILTRAGE (skip les traités)
let remaining = checkpoint.get_remaining(&all_paths);
// → Retourne seulement les 2785 non traités
```

## Logs attendus

Quand tu lances `process_batch_robust` maintenant:

```
INFO: 🔍 Detected 538 archives already extracted from previous run
INFO: Cleaning up extracted folders...
INFO: Cleanup complete: 538 extracted folders removed
INFO: Starting new checkpoint session: dazfinder_2024
INFO: ✅ Pre-marked 538 already extracted archives as processed
INFO: Skipping 538 already processed items
INFO: Starting robust batch processing of 2785 remaining items (total: 3323)
```

## Comment tester

### Option 1: Avec les vraies données

```typescript
// Frontend
import { processBatchRobust } from '$lib/api/checkpoint';

const allPaths = [/* tes 3323 paths */];

const result = await processBatchRobust(allPaths, 'dazfinder_recovery');

// Devrait automatiquement:
// - Détecter 538 déjà faits
// - Ne traiter que 2785 restants
```

### Option 2: Simuler avec test

1. **Créer des faux _extracted**:
```powershell
# Simule 3 archives déjà extraites
$temp = "$env:TEMP\FileManagerDaz\downloads_import"
mkdir "$temp\archive1_extracted"
mkdir "$temp\archive2_extracted"
mkdir "$temp\archive3_extracted"
```

2. **Lancer batch avec ces 3 + autres**:
```typescript
const paths = [
  'C:/test/archive1.zip',  // Devrait être skippé (déjà extrait)
  'C:/test/archive2.rar',  // Devrait être skippé
  'C:/test/archive3.7z',   // Devrait être skippé
  'C:/test/archive4.zip',  // Devrait être traité (nouveau)
];

await processBatchRobust(paths, 'test_detection');
```

3. **Vérifier les logs**:
```
🔍 Detected 3 archives already extracted
✅ Pre-marked 3 already extracted archives as processed
Skipping 3 already processed items
Starting robust batch processing of 1 remaining items
```

## Scénario réel avec DazFinder

```powershell
# Étape 1: Vérifier l'état actuel
Get-ChildItem "$env:TEMP\FileManagerDaz\downloads_import" | 
  Where-Object Name -like '*_extracted' | 
  Measure-Object | 
  Select-Object -ExpandProperty Count
# Devrait afficher: 538

# Étape 2: Lancer le batch
# (Via UI ou commande)
```

Le système va automatiquement:
1. ✅ Détecter les 538 `*_extracted`
2. ✅ Matcher avec les paths sources
3. ✅ Les marquer comme "processed" dans le checkpoint
4. ✅ Nettoyer les 538 dossiers
5. ✅ Ne traiter QUE les 2785 restants

## Avantages

### Avant (sans detection)
❌ Retraiterait les 538 déjà extraits  
❌ Doublon dans la lib  
❌ Perte de temps  

### Après (avec detection)
✅ Skip automatique des 538  
✅ Pas de doublon  
✅ Gain de temps (~2-3h selon taille)  
✅ Fonctionne même sans checkpoint existant  

## Code de détection

```rust
pub fn detect_already_extracted(temp_dir: &Path, source_paths: &[PathBuf]) -> Vec<PathBuf> {
    // 1. Scanner le temp dir
    let entries = fs::read_dir(temp_dir)?;
    
    // 2. Trouver tous les *_extracted
    let mut extracted_names = HashSet::new();
    for entry in entries.flatten() {
        if entry.path().file_name().ends_with("_extracted") {
            let original_name = name.trim_end_matches("_extracted");
            extracted_names.insert(original_name);
        }
    }
    
    // 3. Matcher avec les paths sources
    let mut already_extracted = Vec::new();
    for source_path in source_paths {
        if extracted_names.contains(source_path.file_stem()) {
            already_extracted.push(source_path.clone());
        }
    }
    
    already_extracted
}
```

## Flowchart

```
Start Batch
    ↓
Scan temp dir
    ↓
Find *_extracted folders (538)
    ↓
Extract original names
    ↓
Match with source paths
    ↓
Pre-mark in checkpoint (538 marked)
    ↓
Cleanup temp (538 removed)
    ↓
Filter remaining (2785 left)
    ↓
Process only remaining
    ↓
Success!
```

## Notes importantes

1. **Détection AVANT cleanup** = On peut identifier les archives même après redémarrage
2. **Matching par file stem** = Fonctionne avec `.zip`, `.rar`, `.7z`, etc.
3. **Pas besoin de checkpoint existant** = Fonctionne même sans checkpoint précédent
4. **Pas de doublon** = Les 538 ne seront jamais retraités

## Prochaine utilisation

Maintenant, tu peux:
1. Lancer `process_batch_robust` avec TOUS les 3323 paths
2. Le système détectera automatiquement les 538 déjà faits
3. Ne traitera que les 2785 restants
4. Si crash à nouveau, checkpoint permettra de reprendre

**Plus jamais stuck !** 🎯
