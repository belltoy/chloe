# publish refuses to run on an unsigned artifact
.PHONY: build sign publish clean
build:
	mkdir -p dist && echo "artifact" > dist/app.tar
sign: build
	touch .signed
publish: build
	@test -f .signed || { echo "error: unsigned artifact" >&2; exit 1; }
	@echo "published $$(cat dist/app.tar)"
clean:
	rm -rf dist .signed
