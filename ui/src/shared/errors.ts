// Messages d'erreur venus du launcher (Rust), écrits en français. En anglais, chaque morceau
// connu est remplacé par sa traduction ; un message composé (« Impossible de lancer l'outil :
// erreur réseau : … ») est traduit morceau par morceau. Le détail technique (erreur de
// Windows, adresse…) reste tel quel.

import type { Lang } from './ipc';

// Du plus long au plus court à l'usage : un morceau long passe avant ceux qu'il contient.
const EN: [string, string][] = [
  // Installation, téléchargement
  ["Cet outil n'a pas d'empreinte SHA-256 dans le catalogue : installation refusée par sécurité.", 'This tool has no SHA-256 hash in the catalogue: installation refused for safety.'],
  ["l'empreinte SHA-256 du fichier ne correspond pas à celle publiée : fichier supprimé, il ne sera pas lancé", 'the file’s SHA-256 hash does not match the published one: file deleted, it will not be run'],
  ["cet installateur demande les droits administrateur : le launcher n'installe que pour l'utilisateur", 'this installer asks for administrator rights: the launcher only installs for the current user'],
  ["désinstallation automatique impossible pour cet outil : utilise « Applications installées » dans les paramètres de Windows", 'automatic uninstall is not possible for this tool: use “Installed apps” in Windows Settings'],
  ['téléchargement arrêté et supprimé', 'download stopped and deleted'],
  ['fichier plus gros que prévu', 'file larger than expected'],
  ["Cet outil n'est pas encore téléchargeable", 'This tool cannot be downloaded yet'],
  ['la désinstallation n\'est pas confirmée après', 'uninstall not confirmed after'],
  ['commande de désinstallation illisible', 'unreadable uninstall command'],
  ['dossier de téléchargement impossible à vider', 'download folder could not be emptied'],
  ["Impossible d'ouvrir le dossier de téléchargement", 'Could not open the download folder'],
  ['Impossible de démarrer l\'opération', 'Could not start the operation'],
  ['Outil introuvable dans le registre', 'Tool not found in the registry'],
  ['Outil inconnu du catalogue', 'Tool unknown to the catalogue'],
  ['Outil non installé', 'Tool not installed'],
  ['Exécutable principal inconnu', 'Main executable unknown'],
  ['téléchargement annulé', 'download cancelled'],
  ['renommage impossible', 'rename failed'],
  ["s'est arrêté (code", 'stopped (code'],
  // Lancement et fermeture des outils
  ["Ce logiciel n'a pas été lancé par le launcher : il ne peut pas le fermer.", 'This program was not started by the launcher: it cannot close it.'],
  ["Cet outil n'est ni installé ni disponible en version web.", 'This tool is neither installed nor available as a web version.'],
  ["Impossible de lancer l'outil", 'Could not start the tool'],
  ["Impossible de démarrer l'outil.", 'Could not start the tool.'],
  ['Cet outil est déjà lancé.', 'This tool is already running.'],
  ["Demande d'abord une fermeture normale.", 'Ask for a normal close first.'],
  ['Arrêt forcé impossible', 'Force stop failed'],
  ['impossible de lancer', 'could not start'],
  // Versions web, barre, dispositions
  ["Cet outil n'a pas de version web.", 'This tool has no web version.'],
  ['Adresse de la version web invalide.', 'Invalid web version address.'],
  ['La version web doit être en HTTPS.', 'The web version must use HTTPS.'],
  ['Outil introuvable dans le catalogue.', 'Tool not found in the catalogue.'],
  ["Identifiant d'outil invalide.", 'Invalid tool identifier.'],
  ['Fenêtre du widget introuvable.', 'Widget window not found.'],
  ['Fenêtre du widget non créée', 'Widget window not created'],
  ['Vues du widget non créées', 'Widget views not created'],
  ['Widget non affiché', 'Widget not shown'],
  ["Barre d'outils non créée", 'Toolbar not created'],
  ["Barre d'outils introuvable", 'Toolbar not found'],
  ['Commande réservée aux barres de widget.', 'Command reserved for widget bars.'],
  ["Commande réservée à la barre d'outils.", 'Command reserved for the toolbar.'],
  ['Outil retiré. Remets-le depuis « Mes plugins » ou clic droit sur « + ».', 'Tool removed. Put it back from “My plugins” or right-click “+”.'],
  ['Dispositions non enregistrées', 'Layouts not saved'],
  ['Dispositions indisponibles.', 'Layouts unavailable.'],
  ['Disposition introuvable.', 'Layout not found.'],
  ['État des widgets indisponible.', 'Widget state unavailable.'],
  // Réglages, mises à jour, liens
  ['Impossible de modifier le lancement au démarrage', 'Could not change start with Windows'],
  ["Impossible d'enregistrer les réglages", 'Could not save the settings'],
  ["La mise à jour automatique n'est pas configurée.", 'Automatic update is not configured.'],
  ['Le launcher est déjà à jour.', 'The launcher is already up to date.'],
  ['Mise à jour déjà en cours.', 'Update already in progress.'],
  ['Mise à jour refusée', 'Update refused'],
  ["Impossible d'ouvrir le lien", 'Could not open the link'],
  ['Lien refusé', 'Link refused'],
  ['Adresse refusée', 'Address refused'],
  ['exécutable introuvable', 'executable not found'],
  ['État indisponible.', 'State unavailable.'],
  // Réseau et adresses
  [' inconnu : une nouvelle version du launcher est nécessaire', ' unknown: a newer launcher version is required'],
  ['une nouvelle version du launcher est nécessaire', 'a newer launcher version is required'],
  ['redirection vers un hôte non autorisé', 'redirect to a host that is not allowed'],
  ['adresse avec port ou identifiants refusée', 'address with port or credentials refused'],
  ['adresse hors de la liste autorisée', 'address outside the allowed list'],
  ['adresse non HTTPS refusée', 'non-HTTPS address refused'],
  ['adresse invalide', 'invalid address'],
  ['le serveur a répondu', 'the server answered'],
  ["erreur d'entrée/sortie", 'input/output error'],
  ["erreur d'écriture", 'write error'],
  ['erreur réseau', 'network error'],
  ['JSON illisible', 'unreadable JSON'],
  ['JSON invalide', 'invalid JSON'],
  ['format invalide', 'invalid format'],
];

/** Message d'erreur du launcher dans la langue de l'interface. */
export function translateError(lang: Lang, message: string): string {
  let text = message.replace(/^Error:\s*/, '');
  if (lang === 'fr') return text;
  for (const [fr, en] of EN) {
    if (text.includes(fr)) text = text.split(fr).join(en);
  }
  // Deux-points à l'anglaise.
  return text.replace(/ : /g, ': ');
}
