import neutrino

app = neutrino.App()

@app.route("/add")
def add(x, y):
    return x + y

@app.route("/subtract")
def subtract(x, y):
    return x - y

@app.route("/multiply")
def multiply(x, y):
    return x * y