FROM python:3.12-slim

LABEL org.opencontainers.image.title="EVIDENT workflow base"
LABEL org.opencontainers.image.description="Lightweight runner for EVIDENT manifest and documentation checks"

WORKDIR /workspace

RUN pip install --no-cache-dir PyYAML==6.0.2

COPY workflow/validate_manifest.py /usr/local/bin/evident-validate
RUN chmod +x /usr/local/bin/evident-validate

CMD ["evident-validate", "evident.yaml"]
