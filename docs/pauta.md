# Pauta de entrevista natural de negocio para OpenSpec

> Objetivo: tener conversación corta, natural y suficiente para convertirla luego en artefactos OpenSpec (`proposal.md`, `specs/`, `design.md`, `tasks.md`).
>
> Regla: empezar por negocio, no por solución. Hacer pocas preguntas base. Repreguntar solo cuando haga falta para sacar reglas, alcance, excepciones o criterios de aceptación.

---

## 0. Datos base

- **Fecha:**
- **Entrevistado/a:**
- **Rol / área:**
- **Entrevistador/a:**
- **Iniciativa / nombre tentativo:**

---

## 1. Guion base de entrevista

> Estas son las preguntas mínimas. Si con esto queda claro problema, alcance, reglas y éxito, no hace falta profundizar más.

### 1. Apertura: problema y contexto

- ¿Qué necesitás resolver y por qué importa ahora?
- ¿Qué pasa hoy que te genera dolor, riesgo o pérdida de oportunidad?

### 2. Resultado esperado

- Si esto sale bien, ¿qué debería pasar distinto para negocio o para usuario?
- ¿Cómo sabrías que quedó bien?

### 3. Actores

- ¿Quiénes participan en este proceso y a quién impacta más?

### 4. Flujo actual

- Contame cómo se resuelve hoy, de punta a punta.
- ¿Dónde se traba, demora, duplica o falla?

### 5. Alcance inicial

- Para una primera versión, ¿qué sí o sí debería incluir?
- ¿Qué preferís dejar afuera por ahora?

### 6. Reglas clave

- ¿Qué condiciones o reglas de negocio siempre se tienen que cumplir?
- ¿Qué casos deberían bloquearse o tratarse distinto?

### 7. Cierre con ejemplos

- Dame 2 o 3 ejemplos de cuándo dirías “esto quedó bien”.
- Dame 1 o 2 ejemplos de cuándo dirías “esto no debería pasar”.

---

## 2. Repreguntas solo si hacen falta

> Usar estas repreguntas cuando respuesta quedó vaga, apareció riesgo, o surgió tema que impacta especificación.

### Si falta claridad en objetivo

- ¿Qué cambia concretamente para negocio?
- ¿Qué pasa si no hacemos nada?

### Si hay varios actores

- ¿Todos hacen lo mismo o hay permisos/decisiones distintas?
- ¿Quién aprueba o define excepción?

### Si aparecen reglas o validaciones

- ¿Eso aplica siempre o solo en algunos casos?
- ¿Qué pasa cuando regla no se cumple?
- ¿Hay excepciones? ¿Quién puede autorizarlas?

### Si aparecen integraciones o dependencias

- ¿Depende de otro sistema, equipo o proveedor?
- ¿Qué pasa si eso falla o responde tarde?

### Si aparece dato sensible o compliance

- ¿Hay restricciones legales, auditoría o acceso restringido?

### Si alcance está inflado

- Si hubiera que salir con versión mínima, ¿qué dejarías para después?

---

## 3. Notas de entrevista

### Problema de negocio

_

### Resultado esperado

_

### Actores clave

_

### Flujo actual resumido

_

### Alcance primera versión

_

### No alcance

_

### Reglas de negocio críticas

1. 
2. 
3. 

### Casos que deben bloquearse o tratarse distinto

1. 
2. 
3. 

### Ejemplos de éxito

1. 
2. 
3. 

### Ejemplos de falla

1. 
2. 

### Dependencias o integraciones

1. 
2. 
3. 

---

## 4. Consideraciones técnicas del entrevistador

> Completar aparte. No mezclar con relato de negocio salvo que haga falta aclarar limitación real.

- sistemas involucrados detectados
- módulos probablemente afectados
- dependencias técnicas
- riesgos de implementación
- dudas para validar con equipo técnico
- supuestos que no conviene convertir todavía en requisito
- oportunidades de dividir alcance en cambios más chicos

### Notas técnicas

_

---

## 5. Resumen mínimo para OpenSpec

> Si esta sección queda completa, ya debería alcanzar para pasar a `/opsx:explore` o `/opsx:propose`.

### Problema principal

_

### Objetivo de negocio

_

### Alcance inicial

_

### Reglas críticas

1. 
2. 
3. 

### Casos de aceptación

1. 
2. 
3. 

### Casos que deben fallar o bloquearse

1. 
2. 

### Restricciones o dependencias relevantes

_
