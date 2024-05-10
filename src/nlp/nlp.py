import esupar
import os
import hashlib
import pickle
from pathlib import Path

# god i fucking hate windows
os.environ["NLP_LAZY"] = "1"
os.environ["NLP_CACHE"] = "1"

def load_model():
    global nlp
    nlp = esupar.load("ja_large")

CACHE = bool(os.environ.get("NLP_CACHE"))
if CACHE:
    Path(".nlp_cache/").mkdir(exist_ok=True)
if not os.environ.get("NLP_LAZY"):
    load_model()
else:
    nlp = None

def analyze(input: str):
    if CACHE:
        hash = hashlib.md5(bytes(input, encoding='utf8')).hexdigest()
        try:
            with open(f".nlp_cache/{hash}.pickle", "rb") as file:
                return pickle.load(file)
        except FileNotFoundError:
            pass
    if nlp is None:
        load_model()
    analyzed = nlp(input)
    if CACHE:
        with open(f".nlp_cache/{hash}.pickle", "wb+") as file:
            pickle.dump(analyzed, file)
    return analyzed