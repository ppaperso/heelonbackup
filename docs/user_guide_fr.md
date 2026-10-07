# Guide Utilisateur HeelonBackup

> **Note** : Ceci est la version française du guide utilisateur. Pour la version anglaise, voir [user_guide.md](user_guide.md).

## Table des Matières

1. [Introduction](#-introduction)
2. [Installation](#-installation)
3. [Configuration](#-configuration)
4. [Utilisation de Base](#-utilisation-de-base)
5. [Fonctionnalités Avancées](#-fonctionnalités-avancées)
6. [Dépannage](#-dépannage)
7. [Considérations de Sécurité](#-considérations-de-sécurité)

## 📖 Introduction

HeelonBackup est un outil de sauvegarde en ligne de commande conçu pour les utilisateurs Linux qui souhaitent effectuer des sauvegardes fiables de leurs données vers un NAS Synology (ou tout stockage compatible SMB) via le protocole SMB.

### Fonctionnalités Clés

- **Sauvegardes Fiables** : Vérification de l'intégrité des données via checksums SHA-256
- **Transfers Parallèles** : Utilisation de plusieurs sessions smbclient pour maximiser les performances
- **Flexible** : Répertoires sources et motifs d'exclusion personnalisables
- **Transparent** : Rapports de statut clairs et suivi de progression
- **Restauration** : Possibilité de restaurer des fichiers individuels ou des sauvegardes complètes

### Cas d'Utilisation

- Sauvegarde de laptop personnel vers NAS Synology
- Sauvegarde de serveur domestique
- Sauvegarde de documents importants
- Sauvegarde de fichiers multimédias (photos, vidéos, musique)
- Sauvegarde de fichiers de configuration

## 🚀 Installation

### Prérequis

Avant d'installer HeelonBackup, assurez-vous d'avoir les éléments suivants :

1. **Système Linux** : Fedora 44+ ou toute distribution récente avec glibc 2.34+
2. **Compilateur Rust** : Dernière version stable (Rust 2024 edition)
3. **Client SMB** : `smbclient` doit être installé

### Installation des Prérequis

#### Sur Fedora/RHEL/CentOS :
```bash
sudo dnf install samba-client rust cargo git
```

#### Sur Debian/Ubuntu :
```bash
sudo apt-get install smbclient rust cargo git
```

#### Sur Arch Linux :
```bash
sudo pacman -S smbclient rust git
```

### Installer HeelonBackup

#### Méthode 1 : Depuis les sources (Recommandé)

```bash
# Cloner le dépôt
git clone https://github.com/heelon/heelonbackup.git
cd heelonbackup

# Construire en mode release (optimisé pour la production)
cargo build --release

# Installer le binaire globalement
sudo cp target/release/heelonbackup /usr/local/bin/

# Vérifier l'installation
heelonbackup --version
```

#### Méthode 2 : En utilisant Cargo

```bash
cargo install --git https://github.com/heelon/heelonbackup.git
```

#### Méthode 3 : Téléchargement manuel

1. Télécharger le binaire de la dernière version depuis GitHub
2. Le rendre exécutable : `chmod +x heelonbackup`
3. Le placer dans votre PATH (par exemple, `/usr/local/bin/`)

### Vérifier l'installation

```bash
heelonbackup --help
```

Cela devrait afficher le message d'aide avec toutes les commandes disponibles.

## ⚙️ Configuration

### Configuration rapide

La manière la plus simple de commencer est de laisser HeelonBackup créer une configuration par défaut :

```bash
heelonbackup config init
```

Cela crée un fichier de configuration à `~/.config/heelonbackup/config.json`.

### Emplacement du fichier de configuration

Par défaut, HeelonBackup recherche son fichier de configuration à :

```
~/.config/heelonbackup/config.json
```

Vous pouvez spécifier un fichier de configuration différent en utilisant l'option `--config` ou `-c` :

```bash
heelonbackup --config /chemin/vers/votre/config.json backup
```

### Options de Configuration

Voici une explication détaillée de toutes les options de configuration :

#### Configuration SMB (`smb`)

| Option | Type | Obligatoire | Défaut | Description |
|--------|------|------------|--------|-------------|
| `url` | chaîne | Oui | - | URL du partage SMB (par ex., `smb://192.168.1.100/backup`) |
| `username` | chaîne | Oui | - | Nom d'utilisateur pour l'authentification SMB |
| `password` | chaîne | Non | null | Mot de passe (optionnel, préférer la variable d'environnement) |
| `workgroup` | chaîne | Non | null | Nom du groupe de travail ou domaine |
| `timeout` | entier | Non | 30 | Délai d'attente de connexion en secondes |
| `encrypt` | booléen | Non | true | Exiger le chiffrement SMB3 |

**Exemple :**
```json
"smb": {
  "url": "smb://192.168.1.100/backup",
  "username": "utilisateur_sauvegarde",
  "workgroup": "GROUPEDETRAVAIL",
  "timeout": 60,
  "encrypt": true
}
```

#### Configuration de Sauvegarde (`backup`)

| Option | Type | Obligatoire | Défaut | Description |
|--------|------|------------|--------|-------------|
| `sources` | tableau | Non | ["/home"] | Répertoires à sauvegarder |
| `excludes` | tableau | Non | [] | Motifs à exclure (syntaxe glob) |
| `max_file_size` | entier | Non | 0 | Taille maximale des fichiers en octets (0 = illimité) |
| `workers` | entier | Non | Nombre de CPU | Nombre de workers parallèles |

**Exemple :**
```json
"backup": {
  "sources": ["/home/user/Documents", "/home/user/Images"],
  "excludes": ["*.cache*", "*.thumbnails*", ".Corbeille*"],
  "max_file_size": 1073741824,
  "workers": 4
}
```

#### Configuration de Stockage (`storage`)

| Option | Type | Obligatoire | Défaut | Description |
|--------|------|------------|--------|-------------|
| `base_dir` | chaîne | Non | "heelonbackup" | Répertoire de base sur le NAS pour les sauvegardes |
| `retention` | entier | Non | 5 | Nombre de sauvegardes à conserver |

**Exemple :**
```json
"storage": {
  "base_dir": "mes_sauvegardes",
  "retention": 10
}
```

### Variables d'Environnement

HeelonBackup prend en charge les variables d'environnement suivantes :

#### `HEELONBACKUP_SMB_PASSWORD`

Le mot de passe SMB peut être fourni via cette variable d'environnement au lieu de le stocker dans le fichier de configuration :

```bash
# Définir le mot de passe pour la session en cours
export HEELONBACKUP_SMB_PASSWORD="votre_mot_de_passe_secure"

# Exécuter la sauvegarde avec le mot de passe depuis l'environnement
heelonbackup backup
```

Ceci est la méthode **recommandée** pour fournir le mot de passe, car elle est plus sécurisée que de le stocker dans un fichier de configuration.

#### `RUST_LOG`

Contrôle le niveau de verbosité des logs :

```bash
# Niveau de log debug
export RUST_LOG=heelonbackup=debug

# Niveau info (par défaut)
export RUST_LOG=heelonbackup=info

# Niveau warning uniquement
export RUST_LOG=heelonbackup=warn
```

### Exemple de Configuration Complète

```json
{
  "smb": {
    "url": "smb://192.168.1.100/backup",
    "username": "utilisateur_sauvegarde",
    "workgroup": "GROUPEDETRAVAIL",
    "timeout": 60,
    "encrypt": true
  },
  "backup": {
    "sources": ["/home/user/Documents", "/home/user/Images", "/home/user/Projets"],
    "excludes": [
      "*.cache*",
      "*.thumbnails*",
      ".Corbeille*",
      ".local/share/Trash*",
      "node_modules",
      ".git",
      "target",
      ".vscode",
      ".idea"
    ],
    "max_file_size": 1073741824,
    "workers": 4
  },
  "storage": {
    "base_dir": "heelonbackup",
    "retention": 5
  }
}
```

### Validation de la Configuration

Pour vérifier si votre configuration est valide :

```bash
heelonbackup config check
```

Cela validera le fichier de configuration et signalera toute erreur.

## 🎯 Utilisation de Base

### Réaliser votre première sauvegarde

Une fois que vous avez configuré HeelonBackup, réaliser une sauvegarde est simple :

```bash
heelonbackup backup
```

Cela va :
1. Scanner tous les fichiers dans vos répertoires sources configurés
2. Exclure les fichiers correspondant à vos motifs d'exclusion
3. Transférer les fichiers vers votre NAS via SMB
4. Vérifier l'intégrité des fichiers transférés
5. Créer un manifest avec toutes les métadonnées de sauvegarde

### Suivi de la progression

Pendant la sauvegarde, vous verrez :

- Une barre de progression montrant l'avancement du transfert
- Le nombre de fichiers et la taille
- Le temps estimé restant (ETA)
- La vitesse de transfert

Exemple de sortie :
```
Scanning /home/user... ✓
Found 12,487 files to back up (45.67 GB)
Transferring files... [==========>          ] 1248/12487 4.56 GB/s ETA 2m 30s
```

### Vérification du statut de sauvegarde

Pour voir le statut de votre dernière sauvegarde :

```bash
heelonbackup status
```

Cela affiche :
- Nom de la sauvegarde (horodatage)
- Heures de début et de fin
- Nombre de fichiers sauvegardés
- Taille totale
- Vitesse de transfert
- Statut (complété, complété avec erreurs, échoué)

Pour plus de détails :

```bash
heelonbackup status --detailed
```

### Afficher toutes les sauvegardes

Pour voir une liste de toutes les sauvegardes sur votre NAS :

```bash
heelonbackup list
```

Cela montre toutes les sauvegardes avec leur statut, taille et horodatage.

### Vérification d'une sauvegarde

Pour vous assurer que votre sauvegarde est complète et intacte :

```bash
heelonbackup verify
```

Pour une vérification approfondie qui télécharge et vérifie chaque fichier :

```bash
heelonbackup verify --deep
```

**Note** : La vérification approfondie peut prendre beaucoup de temps et utiliser une bande passante importante.

### Restauration de fichiers

Pour restaurer des fichiers depuis une sauvegarde :

```bash
# Simulation (montre ce qui serait restauré sans le faire réellement)
heelonbackup restore 20261007_143022 /home/user/restaure

# Restauration réelle
heelonbackup restore 20261007_143022 /home/user/restaure --execute

# Restaurer uniquement des fichiers spécifiques
heelonbackup restore 20261007_143022 /home/user/restaure --files "/home/user/Documents/important.pdf"

# Restaurer les fichiers correspondant à un motif
heelonbackup restore 20261007_143022 /home/user/restaure --pattern "*.pdf"
```

Remplacez `20261007_143022` par le nom de votre sauvegarde (horodatage).

## 🚀 Fonctionnalités Avancées

### Sauvegarder des chemins spécifiques

Vous pouvez sauvegarder des répertoires ou fichiers spécifiques qui ne sont pas dans votre configuration :

```bash
heelonbackup backup /home/user/Documents /home/user/Images/Vacances
```

### Exclusions temporaires

Ajoutez des exclusions temporaires pour une sauvegarde spécifique :

```bash
heelonbackup backup --exclude "*.tmp" --exclude "*.log" --exclude "temp/"
```

### Mode Simulation (Dry Run)

Voir ce qui serait sauvegardé sans transférer de fichiers :

```bash
heelonbackup backup --dry-run
```

Ceci est utile pour :
- Tester votre configuration
- Voir quels fichiers seront inclus
- Estimer la taille et le temps de sauvegarde

### Complétions Shell

Générez des complétions shell pour une utilisation plus facile :

#### Bash :
```bash
heelonbackup completions bash > ~/.local/share/bash-completion/completions/heelonbackup
```

#### Zsh :
```bash
heelonbackup completions zsh > ~/.local/share/zsh/site-functions/_heelonbackup
```

#### Fish :
```bash
heelonbackup completions fish > ~/.config/fish/completions/heelonbackup.fish
```

Après avoir ajouté les complétions, redémarrez votre shell ou exécutez :

```bash
# Pour bash
source ~/.local/share/bash-completion/completions/heelonbackup

# Pour zsh
compinit
```

### Gestion de la Configuration

Gérez votre configuration de manière interactive :

```bash
# Créer une configuration par défaut
heelonbackup config init

# Éditer le fichier de configuration
heelonbackup config edit

# Valider la configuration
heelonbackup config check

# Afficher la configuration actuelle
heelonbackup config show
```

## 🔍 Dépannage

### Problèmes Courants et Solutions

#### 1. smbclient non installé

**Erreur :**
```
error: smbclient is not installed
```

**Solution :**
```bash
# Fedora/RHEL
sudo dnf install samba-client

# Debian/Ubuntu
sudo apt-get install smbclient

# Arch Linux
sudo pacman -S smbclient
```

#### 2. Impossible de joindre le NAS

**Erreur :**
```
error: cannot reach the NAS: connection failed
```

**Solution :**
- Vérifiez que le NAS est sous tension et connecté au réseau
- Vérifiez que le service SMB fonctionne sur le NAS
- Vérifiez l'adresse IP ou le nom d'hôte dans votre configuration
- Testez la connexion manuellement :

```bash
smbclient //nas-ip/share -U votre_utilisateur
```

#### 3. Échec de l'authentification

**Erreur :**
```
error: SMB authentication failed: session setup failed
```

**Solution :**
- Vérifiez votre nom d'utilisateur et mot de passe
- Vérifiez si l'utilisateur a accès au partage
- Essayez en utilisant la variable d'environnement :

```bash
export HEELONBACKUP_SMB_PASSWORD="votre_mot_de_passe"
heelonbackup backup
```

- Assurez-vous que le groupe de travail/domaine est correct dans votre configuration

#### 4. Accès refusé

**Erreur :**
```
error: SMB operation failed: NT_STATUS_ACCESS_DENIED
```

**Solution :**
- Vérifiez que l'utilisateur a les permissions d'écriture sur le partage SMB
- Vérifiez les permissions du partage sur votre NAS Synology
- Assurez-vous que l'utilisateur a les droits d'accès corrects

#### 5. Fichier de configuration introuvable

**Erreur :**
```
error: configuration file not found at ~/.config/heelonbackup/config.json
```

**Solution :**
```bash
# Initialiser la configuration
heelonbackup config init

# Ou spécifier un fichier de configuration personnalisé
heelonbackup --config /chemin/vers/config.json backup
```

#### 6. Fichier existe déjà

**Erreur :**
```
error: SMB operation failed: NT_STATUS_OBJECT_NAME_COLLISION
```

**Solution :**
- Cela signifie généralement que le fichier existe déjà sur le NAS
- HeelonBackup devrait gérer cela automatiquement
- Si cela persiste, vérifiez les fichiers en double dans vos sources de sauvegarde

#### 7. Espace insuffisant sur le NAS

**Erreur :**
```
error: SMB operation failed: NT_STATUS_DISK_FULL
```

**Solution :**
- Libérez de l'espace sur votre NAS
- Vérifiez si la politique de rétention fonctionne correctement
- Supprimez manuellement les anciennes sauvegardes si nécessaire

### Mode Debug

Pour un dépannage détaillé, exécutez HeelonBackup avec le logging debug :

```bash
export RUST_LOG=heelonbackup=debug
heelonbackup backup
```

Cela affichera des informations détaillées sur :
- Le chargement de la configuration
- Le scan des fichiers
- La connexion SMB
- Les opérations de transfert
- Les détails des erreurs

### Tester la connexion SMB

Pour tester manuellement votre connexion SMB :

```bash
# Test de connexion de base
smbclient //nas-ip/share -U votre_utilisateur

# Avec mot de passe depuis l'environnement
PASSWD=votre_mot_de_passe smbclient //nas-ip/share -U votre_utilisateur

# Lister les fichiers dans le partage
smbclient //nas-ip/share -U votre_utilisateur -c "ls"
```

## 🛡️ Considérations de Sécurité

### Sécurité du Mot de Passe

**Meilleure Pratique** : Utilisez toujours la variable d'environnement pour votre mot de passe SMB au lieu de le stocker dans le fichier de configuration :

```bash
export HEELONBACKUP_SMB_PASSWORD="votre_mot_de_passe"
```

Cela garantit que le mot de passe n'est pas stocké sur disque et n'est disponible que pour la session en cours.

### Chiffrement

HeelonBackup prend en charge le chiffrement SMB3. Activez-le dans votre configuration :

```json
{
  "smb": {
    "encrypt": true
  }
}
```

Cela chiffrera toutes les données transmises entre votre ordinateur et le NAS.

### Sécurité du Réseau

- Utilisez un réseau sécurisé pour les sauvegardes
- Envisagez d'utiliser un VPN si vous sauvegardez sur Internet
- Maintenez le firmware de votre NAS à jour
- Utilisez des mots de passe forts pour l'accès SMB

### Intégrité des Données

HeelonBackup utilise des checksums SHA-256 pour garantir l'intégrité des données :

- Chaque fichier est haché avant le transfert
- Les checksums sont stockés dans le manifest de sauvegarde
- Les fichiers peuvent être vérifiés après le transfert
- La vérification approfondie télécharge et re-hache les fichiers

### Permissions

- Les fichiers sont sauvegardés avec leurs permissions Unix originales
- Le manifest de sauvegarde stocke les informations de permissions
- Les fichiers restaurés conservent leurs permissions d'origine

## 📈 Conseils de Performance

### Optimiser la Vitesse de Sauvegarde

1. **Augmenter les Workers** : Plus de workers parallèles peuvent améliorer la vitesse de transfert :
   ```json
   "backup": {
     "workers": 8
   }
   ```

2. **Utiliser une Connexion Filaires** : Le Wi-Fi peut être plus lent et moins fiable que l'Ethernet

3. **Planifier Pendant les Heures Creuses** : Exécutez les sauvegardes lorsque l'utilisation du réseau est faible

4. **Exclure les Gros Fichiers Temporaires** : Utilisez `max_file_size` pour ignorer les très gros fichiers qui changent souvent

### Réduire la Taille de Sauvegarde

1. **Ajouter des Motifs d'Exclusion** : Exclure les répertoires dont vous n'avez pas besoin :
   ```json
   "backup": {
     "excludes": ["*.cache*", "*.thumbnails*", "node_modules", ".git"]
   }
   ```

2. **Utiliser max_file_size** : Ignorer les fichiers plus grands qu'une certaine taille :
   ```json
   "backup": {
     "max_file_size": 1073741824  // 1 Go
   }
   ```

### Surveiller les Performances

Utilisez la commande status pour voir les métriques de performance :

```bash
heelonbackup status
```

Cherchez le champ "Rate", qui indique la vitesse de transfert en Mo/s.

## 📊 Référence des Commandes

Pour une documentation complète des commandes, voir [Référence CLI](cli_reference.md).

## 🤝 Obtenir de l'Aide

Si vous rencontrez des problèmes ou avez des questions :

1. **Consultez la Section Dépannage** : La plupart des problèmes courants sont couverts ci-dessus
2. **Examinez les Logs** : Exécutez avec `RUST_LOG=heelonbackup=debug` pour des informations détaillées
3. **Consultez la Documentation** : Voir les autres fichiers dans le répertoire `docs/`
4. **Ouvrez une Issue** : Si c'est un bug ou une demande de fonctionnalité, ouvrez une issue sur GitHub

## 📚 Ressources Additionnelles

- [Référence CLI](cli_reference.md) - Référence complète de la ligne de commande
- [Référence de Configuration](configuration.md) - Options de configuration détaillées
- [Guide de Contribution](../CONTRIBUTING.md) - Comment contribuer au projet
- [Politique de Sécurité](../SECURITY.md) - Politiques de sécurité et signalement des vulnérabilités
