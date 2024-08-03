from dotenv import load_dotenv
load_dotenv()

import sys
import os
VENV = os.environ.get("VENV")
if VENV: sys.path.insert(0, VENV)

import spacy
import hashlib
from pathlib import Path

def load_model():
    global nlp, nlp_small
    nlp = spacy.load('ja_core_news_lg')
    nlp_small = spacy.load('ja_core_news_sm')

CACHE = bool(os.environ.get("NLP_CACHE"))
if CACHE:
    Path(".nlp_cache/").mkdir(exist_ok=True)
if not os.environ.get("NLP_LAZY"):
    load_model()
else:
    nlp_small = None
    nlp = None

def tokenize(docs):
    global nlp_small
    if nlp_small is None:
        load_model()
    if isinstance(docs, str):
        return [token.text for token in nlp_small(docs)]
    else:
        return [list(map(lambda token: token.text, doc)) for doc in nlp_small.pipe(docs)]

def analyze(docs):
    if nlp is None:
        load_model()
    if isinstance(docs, str):
        return postprocess(nlp(docs))
    else:
        return [postprocess(doc) for doc in nlp.pipe(docs)]

def postprocess(doc):
    words = [token.text for token in doc]
    pos_tags = [token.pos_ for token in doc]
    lemmas = [token.lemma_ for token in doc]
    deps = [token.head.i for token in doc]

    return (words, pos_tags, lemmas, deps)