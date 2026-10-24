import sqlite3, pathlib
con = sqlite3.connect(":memory:")
con.executescript(pathlib.Path(__file__).with_name("schema.sql").read_text())
print("migration ok")
