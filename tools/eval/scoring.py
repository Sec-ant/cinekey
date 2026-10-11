"""Title equivalence for media identification scores."""

import unicodedata


def fold_title_accents(title):
    """Compare Latin titles equally with retained or omitted accent marks."""
    characters = []
    latin = False
    for character in unicodedata.normalize("NFD", title):
        if unicodedata.combining(character):
            if not latin:
                characters.append(character)
        else:
            latin = unicodedata.name(character, "").startswith("LATIN ")
            characters.append(character)
    return unicodedata.normalize("NFC", "".join(characters))
