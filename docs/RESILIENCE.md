# Robustesse et Gestion des Erreurs

FileManagerDaz implémente plusieurs mécanismes de robustesse pour gérer les opérations sur de gros volumes d'archives (900+ fichiers).

## Fonctionnalités

### 1. **Retry Automatique avec Exponential Backoff**

En cas d'échec temporaire (réseau, fichier verrouillé, etc.), le système réessaie automatiquement :

```rust
max_extraction_retries: 3 (par défaut)
```

- **Tentative 1** : immédiat
- **Tentative 2** : après 2 secondes
- **Tentative 3** : après 4 secondes
- **Tentative 4** : après 8 secondes

### 2. **Timeout par Opération**

Protection contre les archives corrompues ou trop volumineuses :

```rust
extraction_timeout_seconds: 3600 (1 heure par défaut)
```

Si une extraction dépasse ce délai, elle est annulée et l'opération passe à l'archive suivante.

### 3. **Limite de Taille d'Archive**

Protection mémoire et disque :

```rust
max_archive_size_gb: 0 (illimité par défaut, configurable)
```

Définissez à `10` pour limiter à 10 GB par exemple.

### 4. **Isolation des Erreurs (Skip Corrupted)**

En mode batch, une archive corrompue ne bloque pas tout le traitement :

```rust
skip_corrupted_archives: true (par défaut)
```

- ✅ **Activé** : Les archives corrompues sont enregistrées comme échecs, le reste continue
- ❌ **Désactivé** : Le premier échec arrête tout le batch

### 5. **Validation Pré-Extraction**

Avant de traiter chaque archive :
- Vérification de l'existence du fichier
- Vérification des permissions de lecture
- Vérification de la taille (si limite configurée)

## Configuration

Les paramètres de robustesse sont configurables dans `settings.json` :

```json
{
  "max_extraction_retries": 3,
  "extraction_timeout_seconds": 3600,
  "max_archive_size_gb": 0,
  "skip_corrupted_archives": true
}
```

### Recommandations par Scénario

#### Traitement Rapide (Archive déjà vérifiées)
```json
{
  "max_extraction_retries": 1,
  "extraction_timeout_seconds": 1800,
  "skip_corrupted_archives": false
}
```

#### Traitement Robuste (Archives inconnues, gros volume)
```json
{
  "max_extraction_retries": 5,
  "extraction_timeout_seconds": 7200,
  "max_archive_size_gb": 15,
  "skip_corrupted_archives": true
}
```

#### Mode Debug (Arrêt au premier problème)
```json
{
  "max_extraction_retries": 1,
  "extraction_timeout_seconds": 0,
  "skip_corrupted_archives": false
}
```

## Utilisation

### Commande Robuste (Frontend)

```typescript
import { invoke } from '@tauri-apps/api/core';

// Traiter un batch avec robustesse
const result = await invoke('process_batch_robust', {
  paths: [
    'C:\\Downloads\\archive1.zip',
    'C:\\Downloads\\archive2.rar',
    // ... 900+ fichiers
  ],
  taskId: 'my-batch-001'
});

// Écouter la progression
await listen(`batch-progress-my-batch-001`, (event) => {
  const progress = event.payload;
  console.log(`${progress.completed}/${progress.total} - ${progress.succeeded} OK, ${progress.failed} erreurs`);
});

// Résultat
console.log(`Succès: ${result.data.stats.successful}`);
console.log(`Échecs: ${result.data.stats.failed}`);
console.log(`Fichiers traités: ${result.data.stats.total_files}`);

// Détails des échecs
result.data.failures.forEach(failure => {
  console.error(`❌ ${failure.source_path}: ${failure.error}`);
});
```

### Backend (Rust)

```rust
use crate::core::extractor::{RobustBatchProcessor, ResilienceConfig};
use std::time::Duration;

let config = ResilienceConfig {
    max_retries: 3,
    base_retry_delay: Duration::from_secs(2),
    extraction_timeout: Some(Duration::from_secs(3600)),
    max_archive_size: Some(10 * 1024 * 1024 * 1024), // 10 GB
    skip_corrupted: true,
};

let processor = RobustBatchProcessor::new(config)
    .with_progress(|progress| {
        println!("Progress: {}/{}", progress.completed, progress.total);
    });

let result = processor.process_batch(paths)?;
```

## Événements Émis

### `batch-progress-{taskId}`

Émis régulièrement pendant le traitement :

```typescript
interface BatchProgress {
  total: number;           // Nombre total d'items
  completed: number;       // Items terminés (succès + échecs)
  succeeded: number;       // Items réussis
  failed: number;          // Items échoués
  current_item: string;    // Chemin de l'item en cours
  eta_seconds: number | null; // Temps restant estimé
}
```

## Logs

En cas de problème, les logs détaillent chaque retry :

```
[WARN] Operation failed (attempt 1/3): Archive corrupted. Retrying in 2s
[WARN] Operation failed (attempt 2/3): Archive corrupted. Retrying in 4s
[ERROR] Operation failed after 3 attempts: Archive corrupted
[INFO] Skipping failed item: archive123.zip
```

## Tests

Des tests unitaires garantissent le bon fonctionnement :

```bash
cd src-tauri
cargo test resilience
cargo test batch
```

Tests couverts :
- ✅ Retry avec succès à la 2e tentative
- ✅ Retry épuisé après max_retries
- ✅ Timeout détecté correctement
- ✅ Validation d'archive
- ✅ Traitement batch avec isolation d'erreurs
