import esupar

nlp = esupar.load("KoichiYasuoka/bert-large-japanese-upos")

def analyze(input: str):
    return nlp(input)